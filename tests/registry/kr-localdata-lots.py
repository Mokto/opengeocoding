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
ROAD=re.compile(r'^([가-힣0-9 ]+?(?:로|길)) ('+NUMBER+r')(?=$|\s*[,(])')
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


def road(raw):
    # A building's road address; a unit after the comma or a note in parentheses does not move it.
    value=' '.join((raw or '').split())
    first,sep,rest=value.partition(' ')
    value=ALIASES.get(first,first)+sep+rest
    m=ROAD.match(value)
    if not m:return None
    words=m[1].split()
    if len(words)<2 or not words[0].endswith(('도','광역시','특별시','특별자치시')) or any(not w.endswith(tuple('시군구읍면도')) for w in words[1:-1]):return None
    key=m[1]+' '+m[2]
    return key if len(key)<=256 else None


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


def checked(source,expected):
    digest=hashlib.sha256()
    with source.open('rb') as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b''):digest.update(chunk)
    if digest.hexdigest()!=expected:raise ValueError(f'{source.name} SHA256 mismatch')


def point(raw_x,raw_y):
    if not str(raw_x or '').strip() and not str(raw_y or '').strip():return None
    try:x,y=float(raw_x),float(raw_y)
    except (TypeError,ValueError):return math.nan,math.nan
    if not math.isfinite(x) or not math.isfinite(y) or not x or not y:return math.nan,math.nan
    return x,y


def facilities(rows,counts):
    lots,roads=[],[]
    for raw,raw_road,raw_x,raw_y in rows:
        counts['rows']+=1
        parsed,road_key=address(raw or ''),road(raw_road)
        counts['unparsed_lot_rows']+=not parsed
        counts['unparsed_road_rows']+=not road_key
        coordinate=point(raw_x,raw_y)
        if coordinate is None:counts['missing_coordinate_rows']+=1;continue
        if parsed:lots.append((parsed[0],*coordinate,parsed[1]))
        if road_key:roads.append((road_key,*coordinate,True))
    return lots,roads


def shelters(source):
    counts=defaultdict(int)
    with source.open(encoding='cp949',newline='') as stream:
        reader=csv.DictReader(stream)
        if not {'운영상태','소재지전체주소','도로명전체주소','경도(EPSG4326)','위도(EPSG4326)'}.issubset(reader.fieldnames or []):raise ValueError('Missing shelter columns')
        rows=[]
        for row in reader:
            if None in row:raise ValueError('Malformed shelter CSV row')
            if row['운영상태'] not in ('사용중','사용중지','일시중지'):raise ValueError('Unknown shelter status')
            counts['raw_rows']+=1
            if row['운영상태']=='사용중':rows.append((row['소재지전체주소'],row['도로명전체주소'],row['경도(EPSG4326)'],row['위도(EPSG4326)']))
    return facilities(rows,counts),counts


def protection_zones(source):
    counts=defaultdict(int)
    records=json.loads(source.read_text())['records']
    if any(not {'LNMADR','RDNMADR','LONGITUDE','LATITUDE'}.issubset(r) for r in records):raise ValueError('Missing protection-zone fields')
    counts['raw_rows']=len(records)
    return facilities(((r['LNMADR'],r['RDNMADR'],r['LONGITUDE'],r['LATITUDE']) for r in records),counts),counts


EXTRA={'shelters':shelters,'protection_zones':protection_zones}
EXTRA_SOURCES={'shelters':('행정안전부 민방위대피시설','https://www.data.go.kr/data/15021098/standard.do'),'protection_zones':('어린이보호구역 대상시설','https://www.data.go.kr/data/15012891/standard.do')}


def prepare(source,expected,out,captured,extra=(),extra_captured=None):
    date.fromisoformat(captured)
    checked(source,expected)
    for kind,path,sha in extra:checked(path,sha)
    if extra:date.fromisoformat(extra_captured)
    lots,roads=set(),set();counts=defaultdict(int);invalid_keys=set()
    with source.open(encoding='cp949',newline='') as stream:
        reader=csv.DictReader(stream)
        required={'영업상태코드','지번주소','도로명주소','좌표정보(X)','좌표정보(Y)'}
        if not required.issubset(reader.fieldnames or []):raise ValueError('Missing LOCALDATA columns')
        for row in reader:
            counts['raw_rows']+=1
            if None in row:raise ValueError('Malformed LOCALDATA CSV row')
            status=row['영업상태코드'].strip()
            if status not in ('01','03'):raise ValueError('Unknown LOCALDATA status')
            if status!='01':counts['closed_rows']+=1;continue
            counts['active_rows']+=1
            parsed,road_key=address(row['지번주소']),road(row['도로명주소'])
            counts['unparsed_lot_rows']+=not parsed
            counts['unparsed_road_rows']+=not road_key
            if not parsed and not road_key:continue
            coordinate=point(row['좌표정보(X)'],row['좌표정보(Y)'])
            if coordinate is None:counts['missing_coordinate_rows']+=1;continue
            if math.isnan(coordinate[0]):
                invalid_keys.update(k for k in (parsed and parsed[0],road_key) if k);counts['invalid_coordinate_rows']+=1;continue
            if parsed:lots.add((parsed[0],*coordinate,parsed[1]))
            if road_key:roads.add((road_key,*coordinate,True))
    rows=sorted(lots)+sorted(roads)
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
    is_road=lambda row:bool(ROAD.match(row[0]))
    frames=[(kind,path,sha,*EXTRA[kind](path)) for kind,path,sha in extra]
    kept,quarantined=[],0
    for pick,index in ((lambda row:not is_road(row),0),(is_road,1)):
        # Screening every source together lets cross-source spread and shared points quarantine a key. An address the meal register emits keeps
        # its points, matched on street and number as the builder pairs them: registers spell one place's province or township differently.
        meal,bad=screen([row for row in points if pick(row)])
        if frames:
            combined,bad=screen([row for row in points if pick(row)]+[row for *_,found,counts in frames for row in found[index]])
            pair=lambda key:tuple(key.split()[-2:])
            pairs={pair(key) for key,x,y in meal}
            meal=meal+[row for row in combined if pair(row[0]) not in pairs]
        kept+=meal;quarantined+=bad
    sources=[{'dataset':EXTRA_SOURCES[kind][0],'catalogue':EXTRA_SOURCES[kind][1],'source_sha256':sha,'capture_date':extra_captured,'crs':'EPSG:4326 (source longitude, latitude)','raw_counts':dict(counts)} for kind,path,sha,found,counts in frames]
    kept=sorted({(key,f'{x:.9f}',f'{y:.9f}') for key,x,y in kept})
    content='address\tlon\tlat\n'+''.join(f'{key}\t{x}\t{y}\n' for key,x,y in kept)
    with out.open('wb') as stream,gzip.GzipFile(filename='',mode='wb',fileobj=stream,mtime=0,compresslevel=9) as zipped:zipped.write(content.encode())
    metadata={'dataset':'행정안전부 식품 집단급식소 LOCALDATA','catalogue':'https://www.data.go.kr/data/15096257/fileData.do','source_sha256':expected,'capture_date':captured,'encoding':'CP949','source_crs':'EPSG:5174','prepared_crs':'EPSG:4326 (GIS longitude, latitude)','policy':'Collective meals: active status 01. Additional registers: shelters in use, every protection-zone target facility; their keys are screened together and never replace a collective-meal address key. All sources: positive native bare single-lot addresses, and road addresses whose unit or note follows a comma or parenthesis; lots and roads are screened separately; whole-source shared-coordinate and pre-envelope coordinate-spread quarantine. No service, benchmark or expected-answer filtering. Builder must corroborate every administrative qualifier against the geometry-assigned place/ancestors or one uniquely named indexed child within 5 km, preserving the original point and containing administration.','raw_counts':dict(counts),'source_key_point_style_rows':len(rows),'quarantined_keys':quarantined,'prepared_rows':len(kept),'prepared_road_rows':sum(bool(ROAD.match(key)) for key,x,y in kept),'prepared_unique_addresses':len({key for key,x,y in kept}),'tsv_sha256':hashlib.sha256(content.encode()).hexdigest(),'gzip_sha256':hashlib.sha256(out.read_bytes()).hexdigest(),'additional_sources':sources,'limitations':'Parcel/address reference points, not verified entrances or complete residential coverage.'}
    out.with_suffix('').with_suffix('.json').write_text(json.dumps(metadata,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(metadata,ensure_ascii=False,indent=2),flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('source',type=Path);p.add_argument('--sha256',required=True);p.add_argument('--captured',required=True);p.add_argument('--out',required=True,type=Path)
    for kind in EXTRA:p.add_argument('--'+kind.replace('_','-'),nargs=2,metavar=('FILE','SHA256'))
    p.add_argument('--extra-captured');a=p.parse_args()
    extra=[(kind,Path(v[0]),v[1]) for kind in EXTRA if (v:=getattr(a,kind))]
    prepare(a.source,a.sha256,a.out,a.captured,extra,a.extra_captured)
