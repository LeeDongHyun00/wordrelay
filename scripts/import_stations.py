"""Import factual station names from KRIC's public paginated station directory.
Descriptions are authored templates; raw addresses and third-party prose are not copied.
Run from repository root. Re-running does not duplicate entries.
"""
import gzip, time, concurrent.futures, hashlib, html, json, re, subprocess
from pathlib import Path
BASE='https://www.kric.go.kr/jsp/board/portal/sub05/est/estationList.jsp'
def page(n):
    url=f'{BASE}?pageNo={n}'
    for attempt in range(3):
        blob=subprocess.check_output(['curl','--compressed','-fLs','--retry','2','--max-time','30',url])
        if blob.startswith(b'\x1f\x8b'): blob=gzip.decompress(blob)
        try: raw=blob.decode('utf-8');break
        except UnicodeDecodeError:
            if attempt==2: raise ValueError(f'Invalid response on page {n}: {blob[:24]!r}')
            time.sleep(1)

    rows=[]
    for row in re.findall(r'<tr[^>]*>(.*?)</tr>',raw,re.S):
        cols=[html.unescape(re.sub('<[^>]+>','',c)).strip() for c in re.findall(r'<td[^>]*>(.*?)</td>',row,re.S)]
        if len(cols)>=5 and cols[0].isdigit():
            name=re.sub(r'\([^)]*\)','',cols[1]).strip()
            if re.fullmatch('[가-힣]+',name):
                # Directory names omit the station suffix; 서울 is therefore 서울역.
                label=name if name.endswith('역') else name+'역'
                rows.append({'label':label,'regionOrLine':cols[2],'sourceUrl':url})
    return raw,rows
if '--cached' in __import__('sys').argv:
    rows=json.loads(Path('data/stations.json').read_text());last=69
else:
    raw,first=page(1)
    last=int(re.search(r'현재\s*1\s*페이지\s*/\s*(\d+)',raw).group(1))
    rows=first
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for _,part in pool.map(page,range(2,last+1)): rows.extend(part)
    assert len(rows)>500, 'Incomplete station directory'
rows+=json.loads(Path('data/station-supplements.json').read_text())
records={}
for r in rows: records.setdefault(r['label'],r)
Path('data/stations.json').write_text(json.dumps(list(records.values()),ensure_ascii=False,indent=2)+'\n')
data=json.loads(Path('data/dictionary.json').read_text()); by={e['key']:e for e in data['entries']}
for label,r in records.items():
    sense={'id':'station:'+label,'category':'station-name','definition':f"{r['regionOrLine']}의 철도역 이름. 공식 철도·교통 안내에 등록된 고유 명칭이다.",'acceptanceReason':'실제 철도역의 고유 이름에 ‘역’을 붙인 일상 표기로, 통합 정답 사전에 등록되어 인정합니다. 임의의 단어에 ‘역’을 붙인 표기는 인정하지 않습니다.','source':{'name':r.get('sourceName','국가철도공단 철도산업정보센터 · 철도역 정보'),'url':r['sourceUrl'],'provider':r.get('provider','국가철도공단'),'sourceId':label,'evidence':'official-station-directory','license':'facts-and-editorial-text','retrievedAt':'2026-09-16'}}
    if label not in by:
        by[label]={'id':'word_'+hashlib.sha256(label.encode()).hexdigest()[:20],'label':label,'key':label,'reading':label,'aliases':[],'firstSyllable':label[0],'lastSyllable':label[-1],'length':len(label),'senses':[]}
    entry=by[label];entry['senses']=[s for s in entry['senses'] if s['id']!=sense['id']]+[sense]
data['entries']=sorted(by.values(),key=lambda e:e['key']);data['version']='2026-09-16.'+hashlib.sha256(json.dumps(data['entries'],ensure_ascii=False).encode()).hexdigest()[:12]
Path('data/dictionary.json').write_text(json.dumps(data,ensure_ascii=False,separators=(',',':'))+'\n')
stats=json.loads(Path('data/stats.json').read_text());counts={}
for e in data['entries']:
    for c in {s['category'] for s in e['senses']}: counts[c]=counts.get(c,0)+1
stats.update(version=data['version'],uniqueEntries=len(by),senses=sum(len(e['senses']) for e in by.values()),entriesByCategory=counts,multiCategoryEntries=sum(len({s['category'] for s in e['senses']})>1 for e in by.values()))
Path('data/stats.json').write_text(json.dumps(stats,ensure_ascii=False,indent=2)+'\n')
report=json.loads(Path('data/import-report.json').read_text());report['stationImport']={'source':BASE,'pages':last,'rows':len(rows),'uniqueStations':len(records),'retrievedAt':'2026-09-16'}
Path('data/import-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(stats,ensure_ascii=False))
