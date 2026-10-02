# Content audit report

- Golden: `parity/golden_content.json`
- Status: **PASS**

| check | status | detail |
|---|---|---|
| counts | pass | 12 types compared |
| names_ids_fields | pass | id/name/kind/localized/fields compared |
| tech_trees | pass | 2 trees compared |
| mod_content_name_map | pass | 4 fallbacks compared |
| dangling_refs | pass | dense ids + tech-tree resolution |
| bundle_keys | pass | 667 keys checked, 46 upstream exemptions, 0 missing |
| regions | pass | 33 manifest entries, 2208 regions; 33 authored expectations, 0 soft misses |
