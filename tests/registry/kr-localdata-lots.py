#!/usr/bin/env python3
import argparse
import csv
import gzip
import hashlib
import json
import math
import re
import subprocess
from collections import defaultdict
from datetime import date
from pathlib import Path

NUMBER=r'[1-9][0-9]{0,4}(?:-[1-9][0-9]{0,4})?'
LOT=re.compile(r'^([가-힣0-9 ]+(?:동|리|가))\s+(산\s*)?('+NUMBER+r')(?:번지)?(?=[\s,(]|$)')
ALIASES={'강원도':'강원특별자치도','전라북도':'전북특별자치도'}


def address(raw):
    value=' '.join(raw.split())
    first,sep,rest=value.partition(' ')
    value=ALIASES.get(first,first)+sep+rest
    m=LOT.match(value)
    if not m:return None
    words=m[1].split()
    if not words[0].endswith(('도','광역시','특별시','특별자치시')) or any(not w.endswith(tuple('시군구읍면동리도')) for w in words[:-1]):return None
    key=m[1]+' '+('산' if m[2] else '')+m[3]
    if len(key)>256:return None
    return key,not value[m.end():].strip()


def distance(a,b):
    x,y=a;u,v=b
    q=math.sin(math.radians(v-y)/2)**2+math.cos(math.radians(y))*math.cos(math.radians(v))*math.sin(math.radians(u-x)/2)**2
    return 12742000*math.asin(math.sqrt(min(1,q)))


def screen(rows):
    # ponytail: these flags can reject real multi-parcel sites; parcel geometry would distinguish them.
    by_key=defaultdict(set);by_point=defaultdict(set);bad=set()
    for key,x,y,bare in rows:
        if not math.isfinite(x) or not math.isfinite(y) or not -180<=x<=180 or not -90<=y<=90:bad.add(key);continue
        by_key[key].add((x,y));by_point[x,y].add(key)
    for point,keys in by_point.items():
        if len(keys)>=5:bad.update(keys)
    for key,points in by_key.items():
        lo=(min(p[0] for p in points),min(p[1] for p in points));hi=(max(p[0] for p in points),max(p[1] for p in points))
        if distance(lo,hi)>250:bad.add(key)
    return sorted({(key,x,y) for key,x,y,bare in rows if bare and key not in bad and 124<=x<=132.5 and 32.5<=y<=39.5}),len(bad)


def prepare(source,expected,out,captured):
    date.fromisoformat(captured)
    digest=hashlib.sha256()
    with source.open('rb') as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b''):digest.update(chunk)
    if digest.hexdigest()!=expected:raise ValueError('LOCALDATA capture SHA256 mismatch')
    rows=set();counts=defaultdict(int);invalid_keys=set()
    with source.open(encoding='cp949',newline='') as stream:
        reader=csv.DictReader(stream)
        required={'영업상태코드','지번주소','좌표정보(X)','좌표정보(Y)'}
        if not required.issubset(reader.fieldnames or []):raise ValueError('Missing LOCALDATA columns')
        for row in reader:
            counts['raw_rows']+=1
            if None in row:raise ValueError('Malformed LOCALDATA CSV row')
            status=row['영업상태코드'].strip()
            if status not in ('01','03'):raise ValueError('Unknown LOCALDATA status')
            if status!='01':counts['closed_rows']+=1;continue
            counts['active_rows']+=1
            parsed=address(row['지번주소'])
            if not parsed:counts['unparsed_lot_rows']+=1;continue
            key,bare=parsed
            raw_x,raw_y=row['좌표정보(X)'],row['좌표정보(Y)']
            try:x,y=float(raw_x),float(raw_y)
            except ValueError:
                if raw_x.strip() or raw_y.strip():invalid_keys.add(key);counts['invalid_coordinate_rows']+=1
                else:counts['missing_coordinate_rows']+=1
                continue
            if not math.isfinite(x) or not math.isfinite(y) or not x or not y:
                invalid_keys.add(key);counts['invalid_coordinate_rows']+=1;continue
            rows.add((key,x,y,bare))
    rows=sorted(rows)
    control=subprocess.run(['cs2cs','+init=epsg:5174','+to','+init=epsg:4326','-f','%.9f'],input='200000 500000\n',text=True,capture_output=True,check=True)
    a,b=map(float,control.stdout.split()[:2])
    if abs(a-127.000783539)>1e-7 or abs(b-38.002746025)>1e-7:raise ValueError('Unexpected PROJ datum/axis interpretation')
    transformed=subprocess.run(['cs2cs','+init=epsg:5174','+to','+init=epsg:4326','-f','%.9f'],input=''.join(f'{x} {y}\n' for key,x,y,bare in rows),text=True,capture_output=True,check=True).stdout.splitlines()
    if len(transformed)!=len(rows):raise ValueError('Projection lost row alignment')
    points=[]
    for (key,x,y,bare),line in zip(rows,transformed):
        try:a,b=map(float,line.split()[:2])
        except ValueError:a=b=math.nan
        points.append((key,a,b,bare))
    points.extend((key,math.nan,math.nan,False) for key in invalid_keys)
    kept,quarantined=screen(points)
    content='lot_address\tlon\tlat\n'+''.join(f'{key}\t{x:.9f}\t{y:.9f}\n' for key,x,y in kept)
    with out.open('wb') as stream,gzip.GzipFile(filename='',mode='wb',fileobj=stream,mtime=0,compresslevel=9) as zipped:zipped.write(content.encode())
    metadata={'dataset':'행정안전부 식품 집단급식소 LOCALDATA','catalogue':'https://www.data.go.kr/data/15096257/fileData.do','source_sha256':expected,'capture_date':captured,'encoding':'CP949','source_crs':'EPSG:5174','prepared_crs':'EPSG:4326 (GIS longitude, latitude)','policy':'Active status 01; positive native bare single-lot addresses only; whole-source shared-coordinate and pre-envelope coordinate-spread quarantine. No service, benchmark or expected-answer filtering. Builder must corroborate every administrative qualifier against the geometry-assigned place/ancestors or one uniquely named indexed child within 5 km, preserving the original point and containing administration.','raw_counts':dict(counts),'source_key_point_style_rows':len(rows),'quarantined_keys':quarantined,'prepared_rows':len(kept),'prepared_unique_addresses':len({key for key,x,y in kept}),'tsv_sha256':hashlib.sha256(content.encode()).hexdigest(),'gzip_sha256':hashlib.sha256(out.read_bytes()).hexdigest(),'limitations':'Parcel/address reference points, not verified entrances or complete residential coverage.'}
    out.with_suffix('').with_suffix('.json').write_text(json.dumps(metadata,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(metadata,ensure_ascii=False,indent=2),flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('source',type=Path);p.add_argument('--sha256',required=True);p.add_argument('--captured',required=True);p.add_argument('--out',required=True,type=Path);a=p.parse_args()
    prepare(a.source,a.sha256,a.out,a.captured)
