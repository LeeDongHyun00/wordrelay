# 출처와 이용 조건

## 국어사전 텍스트

저작자: 국립국어원. 저작물: 한국어기초사전.

- 원출처: https://krdict.korean.go.kr/
- 공식 저작권 정책: https://krdict.korean.go.kr/kor/kboardPolicy/copyRightTermsInfo
- 라이선스: Creative Commons 저작자표시-동일조건변경허락 2.0 대한민국 (CC BY-SA 2.0 KR)
- 라이선스 전문: https://creativecommons.org/licenses/by-sa/2.0/kr/legalcode
- 취득 미러: https://github.com/spellcheck-ko/korean-dict-nikl
- 정확한 미러 커밋과 원본 SHA-256: `data/import-report.json`

변경 사항: 명사·한글 표제어 필터링, XML에서 JSON으로 변환, 동형 표제어 통합, 정답 근거 문장과 연결 메타데이터 추가. 원문 뜻풀이를 유지하고 예문·미디어·외국어 번역을 제외했습니다. 국어사전에서 파생한 데이터는 CC BY-SA 2.0 KR로 제공합니다. 재배포 시 국립국어원 출처, 라이선스 링크, 변경 사항을 유지하고 해당 파생 데이터에 동일 조건을 적용하세요.

## 게임 및 작품 정보

- 리그 오브 레전드 공식 목록: https://www.leagueoflegends.com/ko-kr/champions/
- 역할 및 한국어 명칭 데이터 제공: https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/ko_kr/v1/champion-summary.json
- 클래시 로얄 명칭 및 사실 정보: https://github.com/RoyaleAPI/cr-api-data/blob/master/docs/json/cards_i18n.json
- 애니메이션 한국어 제목 확인: https://www.netflix.com/kr/browse/genre/6721

게임 명칭·작품명과 사실 정보를 사용하고 설명 및 인정 근거 문장을 직접 작성했습니다. 이름과 데이터의 출처별 정보는 각 sense에 저장되어 있습니다. 게임 원문 소개문·시놉시스 원문·삽화·로고·음성은 배포하지 않습니다. 사실 정보와 자체 설명이라는 `facts-and-editorial-text` 표시는 제3자 게임 자산에 대한 오픈소스 라이선스를 주장하는 값이 아닙니다. 제작사와의 공식 제휴를 뜻하지 않습니다.

이 패키지에서 새로 작성한 JS·Python 코드 및 문서는 사용자 프로젝트에서 수정·사용할 수 있도록 제공합니다. 국립국어원 파생 데이터의 라이선스 조건은 별도로 유지됩니다.

## 역 이름

국가철도공단 철도산업정보센터 철도역 정보(https://www.kric.go.kr/jsp/board/portal/sub05/est/estationList.jsp)와 서울특별시 교통 안내(https://mediahub.seoul.go.kr/archives/187867)에서 역 이름·노선 사실 정보를 확인했습니다. 설명과 인정 근거는 자체 작성했습니다. 원문 설명·이미지는 포함하지 않습니다. 수집 스냅샷은 2026-09-16 기준이며 모든 역의 현재 운영 여부를 보증하지 않습니다. `stations.json`은 등록 목록, `station-supplements.json`은 추가 확인 목록입니다.

## 우리말샘 확장 사전

저작자: 국립국어원. 원출처: https://opendict.korean.go.kr/
공식 정책: https://opendict.korean.go.kr/service/copyrightPolicy
라이선스: CC BY-SA 2.0 KR, https://creativecommons.org/licenses/by-sa/2.0/kr/legalcode

공개 미러 spellcheck-ko/korean-dict-nikl의 고정 커밋에서 전체 XML을 취득했습니다. 원본 생성 시각, 커밋, 파일별 SHA-256, 필터별 집계는 `opendict-import-report.json`에 기록합니다. 미러는 국립국어원이 운영하는 서비스가 아닙니다.

변경 사항: 명사 성격의 품사와 현대 한글 표기를 추출하고 사전의 붙임표·공백·분석 기호를 제거했습니다. 중복 표기는 하나로 합치고 일반어 뜻을 우선한 대표 뜻풀이 하나를 보존했습니다. 기존 통합 사전에 있는 표기는 기존 뜻풀이를 유지합니다. 방언·북한어·옛말도 해당 품사와 표기 조건을 만족하면 포함하며, 설명의 인정 근거에 구분을 표시합니다. 규범 표기가 따로 있거나 잘못된 표기임을 명시한 뜻풀이는 제외합니다. 예문과 미디어는 포함하지 않습니다. `opendict.json.gz` 파생 데이터는 동일한 CC BY-SA 2.0 KR로 배포합니다.
