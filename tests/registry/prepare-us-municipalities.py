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
    cache.mkdir(parents=True, exist_ok=True)
    for item in manifest['files']:
        name = item['file']
        if not name or Path(name).name != name:
            raise ValueError('Invalid capture filename')
        url = urllib.parse.urlsplit(item['url'])
        if url.scheme != 'https' or url.hostname not in ['tigerweb.geo.census.gov', 'www2.census.gov'] or url.username or url.password:
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
    expected = {item['file'] for item in manifest['files'] if item['file'].endswith('.geojson')}
    if {p.name for p in cache.glob('*.geojson')} != expected:
        raise ValueError('Unexpected geometry captures in cache')
    db = cache / 'municipalities.duckdb'
    rows = cache / 'valid-government-polygons.jsonl'
    query = f"""
LOAD spatial;
SET threads=4;
CREATE OR REPLACE TABLE governments AS
SELECT filename LIKE '%/cousub-%' AS subdivision, GEOID AS geoid, STATE AS state, NAME AS name, geometry, FUNCSTAT
FROM read_json_auto({sql_string(cache / '*.geojson')}, maximum_object_size=134217728, filename=true);
SELECT CASE WHEN count(*)={manifest['captured_entities']} AND count(*)=count(DISTINCT (subdivision,geoid))
 AND count(*) FILTER (WHERE FUNCSTAT='A')=count(*) THEN 1 ELSE error('Incomplete/duplicate/nonactive capture') END FROM governments;
CREATE OR REPLACE TABLE states AS SELECT STATE AS fips, STUSAB AS code FROM read_csv({sql_string(cache / 'state.txt')}, delim='|', header=true, all_varchar=true);
COPY (SELECT subdivision,geoid,concat('US-',s.code) AS region,name,ST_AsGeoJSON(g.geometry)::JSON AS geometry
 FROM governments g JOIN states s ON s.fips=g.state
 WHERE ST_IsValid(g.geometry) AND try_cast(g.state AS INTEGER)<60
 ORDER BY subdivision,geoid) TO {sql_string(rows)} (FORMAT JSON);
"""
    subprocess.run(['duckdb', str(db)], input=query, text=True, check=True)
    records = []
    identities = set()
    quantized = cache / 'quantized.tsv'
    with quantized.open('w') as check:
        check.write('id\twkt\n')
        for line in rows.open():
            row = json.loads(line)
            identity = ('cousub' if row['subdivision'] else 'place') + '/' + row['geoid']
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
    if len(identities) != manifest['entities'] or len(records) != manifest['polygons'] or sha(encoded) != manifest['json_sha256']:
        raise ValueError('Prepared government data differs from the pin')
    if sha(compressed) != manifest['gzip_sha256']:
        raise ValueError('Gzip differs from the pin; check Python/zlib versions')
    out.parent.mkdir(parents=True, exist_ok=True)
    temporary = out.with_name(out.name + '.tmp')
    temporary.write_bytes(compressed)
    temporary.replace(out)
    print(len(identities), 'governments;', len(records), 'polygons;', sha(compressed), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--cache', required=True, type=Path)
    parser.add_argument('--out', type=Path, default=Path('data/us-municipalities.json.gz'))
    parser.add_argument('--manifest', type=Path, default=Path('data/us-municipalities.sources.json'))
    args = parser.parse_args()
    prepare(args.cache, args.out, args.manifest)
