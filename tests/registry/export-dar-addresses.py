#!/usr/bin/env python3
"""Prepare an offline DAR Current JSON snapshot with DuckDB and its spatial extension."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import zipfile

COMMON = ["id_lokalId", "status", "registreringFra", "registreringTil", "virkningFra", "virkningTil"]
COLUMNS = {
    "Adresse": ["adressebetegnelse", "dørbetegnelse", "etagebetegnelse", "husnummer"],
    "Husnummer": ["adgangsadressebetegnelse", "husnummertekst", "adgangspunkt", "navngivenVej", "postnummer", "supplerendeBynavn", "adgangTilBygning", "geoDanmarkBygning"],
    "Adressepunkt": ["position", "oprindelse_tekniskStandard", "oprindelse_nøjagtighedsklasse", "oprindelse_kilde"],
    "NavngivenVej": ["vejnavn", "vejadresseringsnavn", "administreresAfKommune"],
    "NavngivenVejKommunedel": ["navngivenVej", "kommune", "vejkode"],
    "Postnummer": ["navn", "postnr"],
    "SupplerendeBynavn": ["navn"],
}


def encode(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8 * 1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


def identifier(value):
    return '"' + value.replace('"', '""') + '"'


def duckdb(sql):
    sql = "SET autoinstall_known_extensions=false; SET autoload_known_extensions=false; " + sql
    process = subprocess.Popen(["duckdb", "-json", "-c", sql], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        while True:
            try:
                output, error = process.communicate(timeout=20)
                break
            except subprocess.TimeoutExpired:
                print("DAR: DuckDB still processing", flush=True)
    except BaseException:
        process.terminate()
        process.wait()
        raise
    if process.returncode:
        raise ValueError(error.decode(errors="replace"))
    text = output.decode().strip()
    values = []
    while text:
        value, end = json.JSONDecoder().raw_decode(text)
        values.extend(value)
        text = text[end:].lstrip()
    return values


def capture(source, out, seed, candidates):
    if candidates <= 0:
        raise ValueError("positive candidate count required")
    manifest = json.loads((source / "MANIFEST.json").read_bytes())
    if (manifest["register"], int(manifest["version"]), manifest["type"], manifest["format"]) != ("DAR", 4, "Current", "JSON"):
        raise ValueError("requires a DAR v4 Current JSON snapshot")
    generation = int(manifest["generation"])
    stamp = manifest["point_in_time"]
    files = {}
    for entry in manifest["files"]:
        entity = entry["entity"]
        name = entry["filename"]
        if entity not in set(COLUMNS) | {"NavngivenVejPostnummer", "NavngivenVejSupplerendeBynavn"} or entity in files or name != f"DAR_V4_{entity}_TotalDownload_json_Current_{generation}.zip":
            raise ValueError("duplicate entity or inconsistent snapshot filename")
        path = source / name
        if path.stat().st_size != entry["bytes"] or digest(path) != entry["sha256"]:
            raise ValueError(f"DAR archive checksum mismatch: {entity}")
        files[entity] = {k: entry[k] for k in ["entity", "filename", "bytes", "sha256"]}
    if not set(COLUMNS) <= files.keys():
        raise ValueError("DAR snapshot lacks a required entity")
    work = out / "work"
    work.mkdir(parents=True, exist_ok=True)
    prefix = f"SET threads=4; SET memory_limit='2GB'; SET temp_directory={literal(work / 'tmp')}; "
    version = duckdb("SELECT version() AS version")[0]["version"]
    stats = {}
    for entity, extra in COLUMNS.items():
        parquet = work / f"{entity}.parquet"
        receipt_path = work / f"{entity}.receipt.json"
        raw = work / f"{entity}.json"
        columns = COMMON + extra
        schema = "{" + ",".join(f"{literal(c)}:'VARCHAR'" for c in columns) + "}"
        live = "8" if entity == "Adressepunkt" else "3"
        current = f"status={literal(live)} AND CAST(registreringFra AS TIMESTAMPTZ)<=TIMESTAMPTZ {literal(stamp)} AND (registreringTil IS NULL OR TIMESTAMPTZ {literal(stamp)}<CAST(registreringTil AS TIMESTAMPTZ)) AND CAST(virkningFra AS TIMESTAMPTZ)<=TIMESTAMPTZ {literal(stamp)} AND (virkningTil IS NULL OR TIMESTAMPTZ {literal(stamp)}<CAST(virkningTil AS TIMESTAMPTZ))"
        sql = prefix + f"CREATE TEMP TABLE raw AS SELECT * FROM read_json({literal(raw)},format='array',columns={schema}); CREATE TEMP TABLE current_rows AS SELECT * FROM raw WHERE {current}; SELECT CASE WHEN count(*)=count(DISTINCT id_lokalId) THEN true ELSE error('duplicate/null current DAR UUID') END FROM current_rows; COPY current_rows TO {literal(str(parquet) + '.part')} (FORMAT PARQUET,COMPRESSION ZSTD); SELECT (SELECT count(*) FROM raw) AS source_rows,count(*) AS current_rows FROM current_rows;"
        identity = {"input_sha256": files[entity]["sha256"], "sql_sha256": hashlib.sha256(sql.encode()).hexdigest(), "duckdb": version}
        if receipt_path.exists():
            receipt = json.loads(receipt_path.read_bytes())
            if any(receipt[k] != v for k, v in identity.items()) or digest(parquet) != receipt["parquet_sha256"]:
                raise ValueError("DAR preparation checkpoint changed; use a new output directory")
            stats[entity] = receipt["counts"]
            continue
        print(f"DAR: extracting and preparing {entity}", flush=True)
        with zipfile.ZipFile(source / files[entity]["filename"]) as archive:
            member = files[entity]["filename"].removesuffix(".zip") + ".json"
            if archive.namelist() != [member]:
                raise ValueError("unexpected DAR ZIP member layout")
            with archive.open(member) as input_file, raw.open("wb") as output_file:
                progress = time.monotonic()
                for chunk in iter(lambda: input_file.read(8 * 1024 * 1024), b""):
                    output_file.write(chunk)
                    if time.monotonic() - progress >= 10:
                        print(f"DAR {entity}: {output_file.tell() // (1024 * 1024)} MiB extracted", flush=True)
                        progress = time.monotonic()
        result = duckdb(sql)
        stats[entity] = result[-1]
        Path(str(parquet) + ".part").replace(parquet)
        receipt_path.write_bytes(encode(dict(identity, counts=stats[entity], parquet_sha256=digest(parquet))))
        raw.unlink()
        print(f"DAR {entity}: {stats[entity]}", flush=True)

    tables = " ".join(f"CREATE TEMP VIEW {identifier(e)} AS SELECT * FROM read_parquet({literal(work / (e + '.parquet'))});" for e in COLUMNS)
    draw = literal(seed)
    candidate_path = work / "candidates.json"
    # ponytail: direct building IDs can split physical-building folds; stricter grouping needs an audited BBR/GeoDanmark crosswalk.
    sql = prefix + "LOAD spatial; " + tables + f"""
    CREATE TEMP TABLE joined AS
    SELECT h.id_lokalId AS id,h.adgangsadressebetegnelse AS building_label,h.husnummertekst AS number,
      r.id_lokalId AS road_id,coalesce(nullif(r.vejadresseringsnavn,''),r.vejnavn) AS street,
      r.administreresAfKommune AS road_authority,n.id_lokalId AS postcode_id,n.postnr AS postcode,n.navn AS postal_town,
      s.id_lokalId AS supplementary_id,s.navn AS supplementary,
      CASE WHEN nullif(h.adgangTilBygning,'') IS NOT NULL THEN 'bbr:'||h.adgangTilBygning
           WHEN nullif(h.geoDanmarkBygning,'') IS NOT NULL THEN 'geodk:'||h.geoDanmarkBygning ELSE 'entrance:'||h.id_lokalId END AS location,
      p.id_lokalId AS point_id,p.status AS point_status,p.position AS position_25832,
      p.oprindelse_tekniskStandard AS technical_standard,p.oprindelse_nøjagtighedsklasse AS accuracy_class,
      p.oprindelse_kilde AS point_source,TRY(ST_GeomFromText(p.position)) AS geometry
    FROM Husnummer h JOIN Adressepunkt p ON p.id_lokalId=h.adgangspunkt
      JOIN NavngivenVej r ON r.id_lokalId=h.navngivenVej
      JOIN Postnummer n ON n.id_lokalId=h.postnummer
      LEFT JOIN SupplerendeBynavn s ON s.id_lokalId=h.supplerendeBynavn
    WHERE p.oprindelse_tekniskStandard IN ('TD','TK','TN') AND p.oprindelse_nøjagtighedsklasse IN ('A','B')
      AND regexp_full_match(h.husnummertekst,'[0-9]+[A-Za-z]?')
      AND try_cast(regexp_extract(h.husnummertekst,'^[0-9]+') AS BIGINT)>0
      AND (h.supplerendeBynavn IS NULL OR s.id_lokalId IS NOT NULL)
      AND EXISTS(SELECT 1 FROM NavngivenVejKommunedel k WHERE k.navngivenVej=r.id_lokalId AND k.kommune=r.administreresAfKommune);
    CREATE TEMP TABLE projected AS SELECT * EXCLUDE(geometry),ST_Transform(geometry,'EPSG:25832','EPSG:4326',always_xy:=true) AS point FROM joined WHERE geometry IS NOT NULL AND NOT ST_IsEmpty(geometry) AND ST_GeometryType(geometry)='POINT';
    CREATE TEMP TABLE eligible AS SELECT * EXCLUDE(point),ST_Y(point) AS lat,ST_X(point) AS lon FROM projected
      WHERE ST_Y(point) BETWEEN 54 AND 58.5 AND ST_X(point) BETWEEN 7.5 AND 16
      AND starts_with(building_label,street||' '||number||',') AND ends_with(building_label,postcode||' '||postal_town)
      AND regexp_full_match(road_authority,'[0-9]{{4}}') AND regexp_full_match(postcode,'[0-9]{{4}}');
    CREATE TEMP TABLE representatives AS SELECT * FROM eligible QUALIFY row_number() OVER(PARTITION BY location ORDER BY id)=1;
    CREATE TEMP TABLE candidates AS SELECT * FROM representatives ORDER BY sha256({draw}||chr(0)||id),id LIMIT {int(candidates)};
    CREATE TEMP TABLE units AS SELECT a.* FROM Adresse a JOIN candidates c ON c.id=a.husnummer
      WHERE starts_with(a.adressebetegnelse,c.street||' '||c.number||',') AND ends_with(a.adressebetegnelse,c.postcode||' '||c.postal_town)
      QUALIFY row_number() OVER(PARTITION BY a.husnummer ORDER BY coalesce(a.dørbetegnelse,'')='',coalesce(a.etagebetegnelse,'')='',a.id_lokalId)=1;
    COPY (SELECT c.*,a.id_lokalId AS unit_id,a.adressebetegnelse AS unit_label,a.etagebetegnelse AS floor,a.dørbetegnelse AS door FROM candidates c LEFT JOIN units a ON a.husnummer=c.id ORDER BY c.id)
      TO {literal(candidate_path)} (FORMAT JSON,ARRAY true);
    SELECT (SELECT count(*) FROM joined) AS quality_joined,(SELECT count(*) FROM projected) AS projected,
      (SELECT count(*) FROM eligible) AS eligible_entrances,(SELECT count(*) FROM representatives) AS location_groups,
      (SELECT count(*) FROM candidates) AS candidates,(SELECT count(*) FROM units) AS candidate_units;
    """
    selection = duckdb(sql)[-1]
    rows = json.loads(candidate_path.read_bytes())
    if not rows:
        raise ValueError("no eligible DAR candidates")
    value = {"capture": {"register": "DAR", "version": 4, "generation": generation, "point_in_time": stamp, "seed": seed, "candidate_limit": candidates, "geometry_crs": "EPSG:4326", "source_crs": "EPSG:25832", "duckdb": version, "entity_counts": stats, "selection_counts": selection}, "sources": list(files.values()), "addresses": rows}
    output = out / "dar.json"
    data = encode(value)
    if output.exists() and output.read_bytes() != data:
        raise ValueError("refusing to overwrite a different final DAR snapshot")
    output.write_bytes(data)
    print(f"{output}: {len(rows)} candidates, {len(data)} bytes, SHA256 {hashlib.sha256(data).hexdigest()}", flush=True)
    return value


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--candidates", type=int, default=20000)
    args = parser.parse_args()
    capture(args.source, args.out, args.seed, args.candidates)
