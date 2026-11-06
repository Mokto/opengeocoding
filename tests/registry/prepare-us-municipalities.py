import argparse
import gzip
import hashlib
import json
import subprocess
import urllib.parse
import urllib.request
from pathlib import Path


def sha(content):
    return hashlib.sha256(content).hexdigest()


def sql_string(value):
    return "'" + str(value).replace("'", "''") + "'"


def prepare(cache, out, manifest_path):
    manifest = json.loads(manifest_path.read_text())
    cdp = manifest.get('kind') == 'cdp'
    cache.mkdir(parents=True, exist_ok=True)
    for item in manifest['files']:
        name = item['file']
        if not name or Path(name).name != name:
            raise ValueError('Invalid source filename')
        url = urllib.parse.urlsplit(item['url'])
        if url.scheme != 'https' or url.hostname != 'www2.census.gov' or url.username or url.password:
            raise ValueError('Invalid Census source URL')
        path = cache / name
        if not path.exists():
            print('Fetch', name, flush=True)
            with urllib.request.urlopen(item['url'], timeout=240) as response:
                content = response.read(128 * 1024 * 1024 + 1)
            if len(content) > 128 * 1024 * 1024 or sha(content) != item['sha256']:
                raise ValueError(f'{name}: source size/hash mismatch')
            temporary = path.with_suffix('.tmp')
            temporary.write_bytes(content)
            temporary.replace(path)
        if sha(path.read_bytes()) != item['sha256']:
            raise ValueError(f'{name}: cached source hash mismatch')
    layers = ['place'] if cdp else ['place', 'cousub']
    shapefiles = sorted(item['file'] for item in manifest['files'] if item['file'].endswith('.zip'))
    if any(f.rsplit('_', 1)[-1][:-4] not in layers for f in shapefiles):
        raise ValueError('Unexpected shapefile in manifest')
    db = cache / ('cdps.duckdb' if cdp else 'municipalities.duckdb')
    rows = cache / ('valid-cdp-polygons.jsonl' if cdp else 'valid-government-polygons.jsonl')
    # A state file holds that state's entities only; checked so a renamed file cannot add another state.
    reads = '\nUNION ALL BY NAME\n'.join(
        f"SELECT {f.rsplit('_', 1)[-1][:-4] == 'cousub'} AS subdivision, {sql_string(f.split('_')[2])} AS file_state, * "
        f"FROM ST_Read({sql_string('/vsizip/' + str(cache / f) + '/' + f[:-4] + '.shp')})"
        for f in shapefiles
    )
    selected = ("FUNCSTAT='S' AND MTFCC='G4210'" if cdp else
                "FUNCSTAT='A' AND (subdivision AND MTFCC='G4040' OR NOT subdivision AND MTFCC='G4110')")
    name = 'NAME' if cdp else 'NAMELSAD'
    query = f"""
LOAD spatial;
SET threads=4;
CREATE OR REPLACE TABLE source AS {reads};
CREATE OR REPLACE TABLE governments AS
SELECT subdivision, GEOID AS geoid, STATEFP AS state, {name} AS name, geom AS geometry FROM source WHERE {selected};
SELECT CASE WHEN count(*) FILTER (WHERE STATEFP<>file_state)=0 THEN 1 ELSE error('Entity outside its state file') END FROM source;
SELECT CASE WHEN count(*)={manifest['captured_entities']} AND count(*)=count(DISTINCT (subdivision,geoid))
 AND count(*) FILTER (WHERE name IS NULL OR trim(name)='')=0
 AND count(*) FILTER (WHERE NOT ST_IsValid(geometry))={manifest['invalid_entities']}
 THEN 1 ELSE error('Incomplete/duplicate/unnamed/unexpected-validity source') END FROM governments;
CREATE OR REPLACE TABLE states AS SELECT STATE AS fips, STUSAB AS code FROM read_csv({sql_string(cache / 'state.txt')}, delim='|', header=true, all_varchar=true);
COPY (SELECT subdivision,geoid,concat('US-',s.code) AS region,name,ST_AsGeoJSON(g.geometry)::JSON AS geometry
 FROM governments g JOIN states s ON s.fips=g.state
 WHERE ST_IsValid(g.geometry) AND try_cast(g.state AS INTEGER)<60
 ORDER BY subdivision,geoid) TO {sql_string(rows)} (FORMAT JSON);
"""
    subprocess.run(['duckdb', str(db)], input=query, text=True, check=True)
    records = []
    identities = set()
    quantized = cache / ('quantized-cdps.tsv' if cdp else 'quantized.tsv')
    with quantized.open('w') as check:
        check.write('id\twkt\n')
        for line in rows.open():
            row = json.loads(line)
            identity = ('cdp' if cdp else 'cousub' if row['subdivision'] else 'place') + '/' + row['geoid']
            if identity in identities:
                raise ValueError('Duplicate source government')
            identities.add(identity)
            geometry = row['geometry']
            if geometry['type'] not in ['Polygon', 'MultiPolygon']:
                raise ValueError('Nonpolygon government')
            polygons = [geometry['coordinates']] if geometry['type'] == 'Polygon' else geometry['coordinates']
            wkt = []
            for polygon in polygons:
                rings = []
                polygon_wkt = []
                for ring in polygon:
                    points = [[round(axis * 1e7) for axis in point] for point in ring]
                    if len(points) < 4 or points[0] != points[-1]:
                        raise ValueError('Unclosed government ring')
                    rings.append([[p[0]-a[0],p[1]-a[1]] for p,a in zip(points, [[0,0]]+points[:-1])])
                    polygon_wkt.append('(' + ','.join(' '.join(f'{v/1e7:.7f}' for v in p) for p in points) + ')')
                records.append([identity, row['region'], row['name'], rings])
                wkt.append('(' + ','.join(polygon_wkt) + ')')
            check.write(identity + '\tMULTIPOLYGON(' + ','.join(wkt) + ')\n')
    query = f"""
LOAD spatial;
SELECT CASE WHEN count(*) FILTER (WHERE NOT ST_IsValid(ST_GeomFromText(wkt)))=0
 THEN 1 ELSE error('Quantization invalidated government geometry') END
FROM read_csv({sql_string(quantized)}, delim='\t', header=true, max_line_size=33554432);
"""
    subprocess.run(['duckdb', str(db)], input=query, text=True, check=True)
    encoded = json.dumps(records, separators=(',', ':'), ensure_ascii=False).encode()
    compressed = gzip.compress(encoded, compresslevel=9, mtime=0)
    print(len(identities), 'entities;', len(records), 'polygons;', sha(encoded), sha(compressed), flush=True)
    if len(identities) != manifest['entities'] or len(records) != manifest['polygons'] or sha(encoded) != manifest['json_sha256']:
        raise ValueError('Prepared boundary data differs from the pin')
    if sha(compressed) != manifest['gzip_sha256']:
        raise ValueError('Gzip differs from the pin; check Python/zlib versions')
    out.parent.mkdir(parents=True, exist_ok=True)
    temporary = out.with_name(out.name + '.tmp')
    temporary.write_bytes(compressed)
    temporary.replace(out)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--cache', required=True, type=Path)
    parser.add_argument('--out', type=Path, default=Path('data/us-municipalities.json.gz'))
    parser.add_argument('--manifest', type=Path, default=Path('data/us-municipalities.sources.json'))
    args = parser.parse_args()
    prepare(args.cache, args.out, args.manifest)
