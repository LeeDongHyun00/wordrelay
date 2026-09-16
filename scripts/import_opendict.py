"""Import noun entries from a pinned, size- and Git-blob-verified NIKL XML mirror.
Usage: python3 scripts/import_opendict.py /path/to/opendict
Keeps the original curated dictionary and writes a compressed supplement.
No examples or multimedia are redistributed. Generated data: CC BY-SA 2.0 KR.
"""
import collections, datetime, gzip, hashlib, json, re, sys, unicodedata
import xml.etree.ElementTree as ET
from pathlib import Path
root=Path(__file__).resolve().parents[1]; source=Path(sys.argv[1])
manifest=json.loads((source/'manifest.json').read_text())
base=json.loads((root/'data/dictionary.json').read_text())
def norm(s):
 return ''.join(c for c in unicodedata.normalize('NFKC',s).lower() if not c.isspace() and not unicodedata.category(c).startswith('P'))
existing={norm(s) for e in base['entries'] for s in [e['label'],e['reading'],*e['aliases']]}
selected={}; counts=collections.Counter(); dates=set(); inputs=[]
for f in manifest['files']:
 p=source/Path(f['path']).name
 assert p.stat().st_size==f['size'], f'Incomplete input: {p}'
 h=hashlib.sha1(f"blob {f['size']}\0".encode());sha=hashlib.sha256()
 with p.open('rb') as stream:
  for chunk in iter(lambda:stream.read(1024*1024),b''):h.update(chunk);sha.update(chunk)
 assert h.hexdigest()==f['sha'],f'Corrupt input: {p}'
 inputs.append({'file':p.name,'bytes':f['size'],'sha256':sha.hexdigest()})
 context=ET.iterparse(p,events=('start','end'));_,xmlroot=next(context)
 for event,e in context:
  if event!='end':continue
  if e.tag=='lastBuildDate':dates.add(e.text)
  if e.tag!='item':continue
  counts['sourceSenses']+=1
  word=e.findtext('wordInfo/word','');pos=e.findtext('senseInfo/pos','');kind=e.findtext('senseInfo/type','')
  label=re.sub(r'[-^\s]','',word)
  definition=e.findtext('senseInfo/definition','').strip();sid=e.findtext('target_code')
  if '명사' not in pos:counts['excludedNonNoun']+=1
  elif not re.fullmatch('[가-힣]+',label):counts['excludedSpellingOrLength']+=1
  elif not definition:counts['excludedNoDefinition']+=1
  elif re.search(r'⇒\s*규범 표기는|[’\']의 잘못',definition):counts['excludedIncorrectSpelling']+=1
  elif label in existing:counts['coveredByBase']+=1
  else:
   priority=(kind!='일반어',int(e.findtext('senseInfo/sense_no','1')))
   if label not in selected or priority<selected[label][0]:
    selected[label]=(priority,{'label':label,'reading':label,'aliases':[],'firstSyllable':label[0],'lastSyllable':label[-1], 'senses':[{'id':'opendict:'+sid,'category':'korean-noun','definition':definition,'acceptanceReason':f'국립국어원 우리말샘에 {pos}({kind})로 수록된 표제어입니다.','partOfSpeech':pos,'wordType':kind,'originalHeadword':word,'source':{'name':'국립국어원 우리말샘','url':'https://opendict.korean.go.kr/dictionary/view?sense_no='+sid}}]})
  e.clear();xmlroot.clear()
 print(p.name,len(selected),flush=True)
entries=[selected[k][1] for k in sorted(selected)]
blob=json.dumps({'entries':entries},ensure_ascii=False,separators=(',',':')).encode()
version='2026-09-16.'+hashlib.sha256(blob).hexdigest()[:12]
with (root/'data/opendict.json.gz').open('wb') as f:
 with gzip.GzipFile(filename='',mode='wb',fileobj=f,mtime=0) as z:z.write(blob)
base['supplements']=['opendict.json.gz'];base['version']=version
(root/'data/dictionary.json').write_text(json.dumps(base,ensure_ascii=False,separators=(',',':'))+'\n')
stats=json.loads((root/'data/stats.json').read_text());allentries=base['entries']+entries
categories=collections.Counter(c for e in allentries for c in {s['category'] for s in e['senses']})
stats.update(version=version,uniqueEntries=len(allentries),senses=sum(len(e['senses']) for e in allentries),entriesByCategory=dict(categories),multiCategoryEntries=sum(len({s['category'] for s in e['senses']})>1 for e in allentries))
(root/'data/stats.json').write_text(json.dumps(stats,ensure_ascii=False,indent=2)+'\n')
report={'repository':'https://github.com/spellcheck-ko/korean-dict-nikl','commit':manifest['commit'],'sourceBuildDates':sorted(dates),'retrievedAt':datetime.date.today().isoformat(),'inputs':inputs,'counts':dict(counts),'addedUniqueEntries':len(entries),'totalUniqueEntries':len(allentries),'policy':'One or more modern Hangul syllables; noun-bearing parts of speech; prefer general-language first sense; preserve curated base; exclude explicitly incorrect spellings, examples and multimedia. One representative sense per added spelling.','license':'CC-BY-SA-2.0-KR'}
(root/'data/opendict-import-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
for word in ['비비','비수']:assert any(e['label']==word for e in allentries),word
print(json.dumps(stats,ensure_ascii=False))
