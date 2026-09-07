Protocol `slice-011c-authenticated-http-v1`; fixture people=50,000 inquiries=46,064 contact facts=52,306 corrections=2,906 inbound=2,843; clock 2026-09-06T12:00:00Z; source hash `8d184a772f58…`; build hash `9a6ef7a50171…`.
Series: 15; late capture reconciliations: 0; sentinel safety: {'forbidden_fixture_sentinels_absent': True, 'required_safe_telemetry_fields_present': True}.

| Case | Series | Arm | Attempts exp/rec/complete | Source evals | c1 p95 / cap | c10 p95 / cap | c20 p95 / cap | Whole-source p95 / max | Enum max | Auth acq p95 / max | Feed acq p95 / max | Feed headroom |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| concentrated_zero_sources | paired_original_zero_source | frozen_original | 68/68/68 | 0 | 726 / 1,250 | 1,260 / 2,500 | 2,372 / 4,500 | – / – | – | 2 / 2 | 1,309 / 1,351 | 649 |
| typical_zero_sources | paired_original_zero_source | frozen_original | 35/35/35 | 0 | 410 / 1,250 | 710 / 2,500 | 1,265 / 4,500 | – / – | – | 2 / 2 | 767 / 774 | 1,226 |
| partial_builtins_zero_sources | paired_original_zero_source | frozen_original | 35/35/35 | 0 | 403 / 1,250 | 647 / 2,500 | 1,098 / 4,500 | – / – | – | 2 / 2 | 662 / 663 | 1,337 |
| empty_builtins_zero_sources | paired_original_zero_source | frozen_original | 35/35/35 | 0 | 415 / 1,250 | 644 / 2,500 | 1,170 / 4,500 | – / – | – | 2 / 2 | 692 / 702 | 1,298 |
| concentrated_zero_sources | final_matrix | final | 68/68/68 | 0 | 235 / 1,250 | 439 / 2,500 | 786 / 4,500 | – / – | 17 | 1 / 2 | 485 / 521 | 1,479 |
| concentrated_one_dense_source | final_matrix | final | 68/68/48 **NO** | 48 | 358 / 1,250 | 2,824 / 2,500 **NO** | 2,788 / 4,500 | 252 / 282 | 20 | 1 / 2 | 2,003 / 2,003 | – |
| concentrated_one_absence_source | final_matrix | final | 68/68/48 **NO** | 48 | 2,177 / 1,250 **NO** | 3,132 / 2,500 **NO** | 2,779 / 4,500 | 152 / 179 | 42 | 1 / 2 | 2,002 / 2,002 | – |
| concentrated_five_overlapping_sources | final_matrix | final | 68/68/48 **NO** | 240 | 2,528 / 1,250 **NO** | 4,012 / 2,500 **NO** | 3,582 / 4,500 | 280 / 380 | 31 | 2 / 2 | 2,004 / 2,004 | – |
| typical_zero_sources | final_matrix | final | 35/35/35 | 0 | 116 / 1,250 | 193 / 2,500 | 341 / 4,500 | – / – | 15 | 1 / 1 | 194 / 218 | 1,782 |
| typical_five_overlapping_sources | final_matrix | final | 35/35/35 | 175 | 137 / 1,250 | 223 / 2,500 | 384 / 4,500 | 11 / 15 | 11 | 1 / 1 | 215 / 230 | 1,770 |
| partial_builtins_zero_sources | final_matrix | final | 35/35/35 | 0 | 8 / 1,250 | 21 / 2,500 | 40 / 4,500 | – / – | 7 | 1 / 1 | 23 / 23 | 1,977 |
| partial_builtins_five_overlapping_sources | final_matrix | final | 35/35/35 | 175 | 481 / 1,250 | 1,042 / 2,500 | 2,045 / 4,500 | 295 / 359 | 45 | 1 / 1 | 1,101 / 1,149 | 851 |
| empty_builtins_zero_sources | final_matrix | final | 35/35/35 | 0 | 6 / 1,250 | 13 / 2,500 | 22 / 4,500 | – / – | 6 | 1 / 1 | 14 / 14 | 1,986 |
| empty_builtins_five_overlapping_sources | final_matrix | final | 35/35/35 | 175 | 461 / 1,250 | 1,072 / 2,500 | 1,928 / 4,500 | 277 / 359 | 20 | 1 / 1 | 1,013 / 1,073 | 927 |
| concentrated_five_overlapping_sources | independent_concentrated_repeat | final | 40/40/20 **NO** | 100 | – | – | 3,683 / 4,500 | 269 / 319 | 70 | 2 / 2 | 2,002 / 2,002 | – |

| Paired zero-source case | Comparison | Payload parity | Hash parity | c1 orig→final (allowed) | c10 orig→final (allowed) | c20 orig→final (allowed) |
|---|---|---|---|---|---|---|
| concentrated_zero_sources | yes | yes | yes | 726→235 (799) | 1,260→439 (1,386) | 2,372→786 (2,609) |
| typical_zero_sources | yes | yes | yes | 410→116 (451) | 710→193 (781) | 1,265→341 (1,392) |
| partial_builtins_zero_sources | yes | yes | yes | 403→8 (443) | 647→21 (712) | 1,098→40 (1,208) |
| empty_builtins_zero_sources | yes | yes | yes | 415→6 (456) | 644→13 (709) | 1,170→22 (1,287) |

Measured attempts: final 522, original 173; warm-up attempts 900; final whole-source evaluations 961.
Series passing the harness's normal-completion gate: 11 of 15.
