# Formations and positional roles in modern professional football (men's and women's, 2018–2026)

Research report, 2026-10-30. Written for the layer-1 (sourced tactics) design of the match engine; the role set in §3 is used in [`../player-brain.md`](../player-brain.md) section 5, and the source keys in §5 are in [`sources.md`](sources.md).

## 0. Method and conventions

- **Where the files are.** The downloads (≈1.3 GB) and scripts were working copies in the research session and are not kept in the repo. File names in §5 refer to them; every source can be fetched again from its URL.
- **Quotes.** Every quote below was checked against the saved text with `findq.py` (whitespace-normalised exact match) or `grep -F`. Quotes found by the two helper agents were checked again the same way.
  - "PDF p." is the page number in the PDF file. The printed page number can differ by one.
  - German and Dutch quotes are verbatim, followed by my translation in brackets.
  - **(paraphrase)** marks text that is not a quote.
  - **[own count]** marks a number I tallied from the source; the source does not state it.
- **Notation.** UEFA and FIFA write formations with the goalkeeper as "1-". I keep that when quoting and drop it in my own text.
  - **IP** = in possession, **OOP** = out of possession.
  - FIFA's post-match reports label the single-pivot 4-3-3 as **4-1-2-3**.
- **Source keys.** Existing project keys are reused where they fit (FIFA-TSG, FIFA-WC22-MB, FIFA-N9-WC22, FFA-NC, FFA-433, KNVB-TF, UEFA-TR24). New keys are in the appendix (§5) and were added to `sources.md` on 2026-10-30.
- **Calibration warning.** The usage numbers below say which shapes and roles the engine must be able to play, and how common each one is. They are **not** calibration targets for an average team. This follows the project's style-variety rule: real teams differ by style.

---

## 1. Formations found, with usage figures

### 1.1 Men's football

#### A. FIFA World Cup 2022: FIFA's official starting formations, 63 matches, 126 line-ups [own count] (FIFA-WC22-PMR)

- **Source.** All 64 FIFA post-match summary reports. They are linked at https://www.fifatrainingcentre.com/en/fwc2022/post-match-summaries/post-match-summary-reports.php, with the pattern `https://fifatrainingcentre.com/media/native/world-cup-2022/report_<id>.pdf`.
- **Label.** Page 2 of each report prints a rotated label for each team. For Netherlands 3-1 USA (report_128068.pdf) it reads "F O R M AT I O N 3 - 4 - 1 - 2" for the Netherlands and "F O R M AT I O N 4 - 1 - 2 - 3" for the USA.
- **Missing match.** Match 39 (Poland v Argentina) prints "F O R M AT I O N N /A" for both teams.
- **How labels were matched to teams.** I used word positions (bounding boxes): the left label belongs to the first-named team. I checked this on report_128068.
- **Files.** The tally script is `formations_scripts/wc22_tally.py`. Its output is `formations_raw/wc22_pmsr_tally.txt`.

| Starting formation (FIFA label) | Line-ups | Share |
|---|---|---|
| 4-1-2-3 (= 4-3-3 with one holding midfielder) | 54 | 43 % |
| 4-2-3-1 | 18 | 14 % |
| 4-4-2 | 14 | 11 % |
| 3-4-3 | 14 | 11 % |
| 5-3-2 | 8 | 6 % |
| 3-4-1-2 | 6 | 5 % |
| 3-5-2 | 4 | 3 % |
| 4-1-4-1 | 3 | 2 % |
| 4-4-1-1 | 3 | 2 % |
| 5-4-1 | 2 | 2 % |

Grouped by family [own count]:

| Family | Line-ups | Share |
|---|---|---|
| 4-3-3 (4-1-2-3, 4-1-4-1) | 57 | 45 % |
| Back three or back five | 34 | 27 % |
| 4-2-3-1 | 18 | 14 % |
| 4-4-2 (4-4-2, 4-4-1-1) | 17 | 13 % |

- **Back three or five, by team.** 18 of 32 teams started at least one match with one. 8 teams did so in most of their matches: Belgium, Costa Rica, Denmark, Netherlands, Qatar, Serbia, Tunisia, Wales.
- **What the label measures.** FIFA does not explain the label for 2022. The FIFA physical analysis says its roles were "based on the tactical systems adopted at the start of games" (FIFA-WWC23-PHYS, women's edition). Treat the labels as starting structures, not as in- or out-of-possession shapes.
- **Team profiles.** The FIFA team profiles flipbook (https://documents.fifatrainingcentre.com/FIFA_WC2022/) has text pages headed "TEAM SHAPE KEY: IN POSSESSION OUT OF POSSESSION". The shapes themselves are graphics only, with no formation text. See §4.

#### B. UEFA EURO 2024 (UEFA-TR24, PDF in §5)

**Attacking (creation-phase) formations.** The table is on PDF p.50, "ATTACKING FORMATIONS", counted in times used. The entries add up to 102, which is 51 matches × 2 teams. That denominator is my inference [own count].

| Shape | Times used | Shape | Times used |
|---|---|---|---|
| 1-3-2-2-3 | 33 | 1-4-1-4-1 | 4 |
| 1-4-3-3 | 27 | 1-4-2-2-2 | 4 |
| 1-4-4-2 | 7 | 1-3-5-2 | 4 |
| 1-4-2-3-1 | 6 | 1-3-4-1-2 | 4 |
| 1-3-4-3 | 6 | 1-4-4-2 (Diamond) | 1 |
| 1-3-4-3 (Diamond) | 5 | 1-5-4-1 | 1 |

- **Shares [own count].** Back-three-based creation shapes: 53 of 102 (52 %). Back-four-based: 49 (48 %).
- **UEFA-TR24 PDF p.50:** "the box midfield or 1-3-2-2-3 set-up. As the table below shows, this was the most commonly seen system in the tournament for sides in the creation phase (used 33 times by teams) with 1-4-3-3 the second most popular approach (27 times)". Also: "It enables the attacking team to cover the full width of the pitch with five players".

**Mid-block shapes.** PDF p.56, "TOP FIVE MID-BLOCK SHAPES", counted as the number of teams that used each shape:

| Shape | Teams |
|---|---|
| 1-4-4-2 | 21 |
| 1-4-3-3 | 18 |
| 1-5-4-1 | 16 |
| 1-5-3-2 | 16 |
| 1-4-2-3-1 | 14 |

UEFA-TR24 PDF p.56: "1-4-4-2 was the most popular with 21 of the 24 teams defending in this shape at least once".

**Per-team SHAPE lines (24 teams), primary shape [own count]:**

| Primary shape | Teams | Which teams |
|---|---|---|
| 4-2-3-1 | 7 | Austria, Belgium, England, Germany, Portugal, Türkiye, Ukraine |
| 4-3-3 | 7 | Albania, Croatia, France, Italy, Netherlands, Slovakia, Spain |
| 4-1-4-1 | 1 | Romania |
| 4-4-2 | 1 | Slovenia |
| Back three or five | 8 | Czechia, Denmark, Georgia, Hungary, Poland, Scotland, Serbia, Switzerland |

- **Back three or five at any stage [own count].** 15 of 24 teams used one at some stage, counting build-up shapes:
  - Albania 1-5-3-2.
  - Belgium and England 1-3-4-2-1.
  - Germany "1-3-4-3 diamond when building play".
  - Netherlands "1-3-2-2-3 in build-up".
  - Türkiye "also 1-5-2-3, 1-5-4-1".
  - Italy, from its key features: "Built with 1-3-2-2-3 in controlled possession".
  - The 8 primary back-three teams listed above.
- **Examples of SHAPE lines with an in-possession or out-of-possession split:**
  - Romania: "1-4-1-4-1; 1-4-2-3-1 in mid/low block"
  - Serbia: "1-3-4-2-1; 1-5-4-1, 1-5-3-2 defensively"
  - Switzerland: "1-3-4-2-1, building with 1-3-2-2-3; 1-5-4-1 v Germany"

**Rest defence.** UEFA-TR24 PDF p.60: "Among the 24 teams, the most common structure was 2+3, which meant two defenders close to the halfway line monitoring the opposition forwards and three more covering the width of the penalty box." For Spain, the full-backs were "narrow with a holding midfielder – usually Rodri – in between".

#### C. UEFA EURO 2020 (UEFA-TR20)

- **Back threes.** PDF p.4: "During UEFA EURO 2020, 14 teams fielded, at some stage, three centre-backs". This follows the earlier tournament, "where 1-4-2-3-1 had been the favourite formation" (printed with a line-break artefact as "1-4-23-1").
  - The report contradicts itself. PDF p.35: "The Dutch and the French were among the 15 teams who, at some stage of the tournament, played with three at the back". So the figure is 14 or 15 of 24.
- **4-4-2.** PDF p.35: "Sweden were alone in deploying a classic 1-4-4-2", against five such teams in 2016 (paraphrase of the same passage).
- **Shape change.** PDF p.35: "Teams moved from five defenders when out of possession to a 3-2-5 shape when attacking".
- **Three attackers.** PDF p.39: "a clear trend towards three attackers".
- I did not extract EURO 2020 per-team shapes. The team pages use a different layout (see §4).

#### D. UEFA Champions League 2023/24, 32 teams (UEFA-UCL24): per-team SHAPE lines

Script: `formations_scripts/shapes.py`. Counts are my own.

**Primary shape:**

| Primary shape | Teams | Which teams |
|---|---|---|
| 4-3-3 | 13 | |
| 4-2-3-1 | 11 | |
| 4-4-2 (incl. diamond) | 3 | Leipzig and Salzburg (diamond), Real Madrid |
| 4-3-2-1 | 1 | Man United |
| Back three | 4 | Atlético 1-3-5-2, Inter 1-3-5-2, Lens 1-3-4-2-1, Union Berlin 1-5-3-2 |

**Out-of-possession shapes named in the same lines** (some teams name more than one):
- **4-4-2 or 4-4-1-1: 10.** Arsenal, Benfica, Braga, Celtic, Feyenoord (at Lazio), Galatasaray, Milan, Real Madrid, Sevilla, Shakhtar.
- **4-1-4-1 or 4-5-1: 10.** Antwerp, Barcelona, Copenhagen, Lazio, Man City, Napoli, Newcastle, Porto, PSV, Real Sociedad.
- **Back five: 7.** Atlético, Copenhagen (v Man City), Crvena zvezda, Inter, Lens, Union, Young Boys.
- **4-1-3-2 or 4-2-3-1: 2.** Leipzig, Salzburg.

**Verbatim examples:**
- Arsenal "1-4-3-3, defending with 1-4-4-2", with the bullet "Built up with 1-3-2-2-3 with full-back (Tomiyasu, Zinchenko) moving into midfield" (PDF p.38).
- Benfica "1-4-2-3-1; shift to 1-4-4-2 out of possession".
- Barcelona "1-4-3-3; also 3-4-3 and 1-3-2-5 on the ball, 1-4-5-1 off it".
- Man City "1-4-2-3-1; 1-4-1-4-1 out of possession".
- PSG: "Shift to back three in build-up with Hakimi pushing up from right-back" (PDF p.49).
- Atlético: "Built up in 1-4-4-2 with a wing-back (Lino) stepping up into midfield" (PDF p.38).

#### E. UCL 2024/25 (UEFA-UCL25), FIFA Club World Cup 2025 and World Cup 2026 articles (FIFA-IFB-CWC25, FIFA-WC26)

These have no per-team shape pages, so the evidence is qualitative.
- **UCL 2024/25:**
  - Inter: "In a rigorous 1-5-3-2 shape" (PDF p.29).
  - Real Madrid's mid-block: "Jude Bellingham edged forward from midfield to form a front two with Kylian Mbappé, creating a 1-4-4-2" (PDF p.28).
  - Aston Villa: "in a 4-4-2, which would become a back five and eventually six" (PDF p.44).
- **World Cup 2026:**
  - Mexico: "in a 1-4-4-2 (diamond) shape without the ball and in a 1-4-2-3-1 with the ball".
  - Sweden's "out-of-possession shape was 1-5-2-3".
  - Cabo Verde: "Operating in a 1-4-5-1 formation".
  - France against Senegal "switched from building in a back four to a back three. By dropping their deep-lying central midfielder, Adrien Rabiot, between the two centre-backs" (FIFA-WC26-B3).
- **The WC 2026 back-three article** asks which kind of build-up three a team uses: "Was it with two centre-backs and a defensive midfielder? Or two centre-backs with one low full-back? Or with three centre-backs?" (FIFA-WC26-B3).

### 1.2 Women's football

#### A. FIFA Women's World Cup 2023: 64 matches, 128 starting line-ups [own count] (FIFA-WWC23-PMR)

- **Source.** FIFA post-match summary reports, page 2 labels, same method as 1.1 A. The extraction was done by a helper agent and I recomputed the tally independently from its per-match list: `formations_raw/women/wwc23_pmsr/wwc23_formations_extracted.txt`.
- **Hub page:** https://www.fifatrainingcentre.com/en/game/tournaments/fifa-womens-world-cup/2023/match-report-hub/post-match-summary-reports.php

| Starting formation | Line-ups | Share |
|---|---|---|
| 4-1-2-3 | 34 | 27 % |
| 4-2-3-1 | 29 | 23 % |
| 4-4-2 | 26 | 20 % |
| 3-4-3 | 11 | 9 % |
| 3-5-2 | 10 | 8 % |
| 5-4-1 | 5 | 4 % |
| 4-4-1-1 | 4 | 3 % |
| 3-4-1-2 | 3 | 2 % |
| 4-5-1 | 2 | 2 % |
| 4-1-4-1 | 2 | 2 % |
| 5-3-2 | 1 | 1 % |
| 4-1-2-1-2 | 1 | 1 % |

- **By family:** 4-3-3 family 38 (30 %), 4-4-2 family 31 (24 %), back three or five 30 (23 %), 4-2-3-1 29 (23 %).
- **Back three or five, by team.** 11 of 32 teams used one at least once. 8 did so in most of their matches: England, Japan, Korea Republic, Netherlands, Panama, Portugal, Republic of Ireland, Vietnam.
- **Modal family per team:** 4-4-2 family 8, back three or five 8, 4-3-3 family 7, 4-2-3-1 7. Two teams were ties (Colombia, Costa Rica).
- **Example label.** The final (report_134412) reads "2 - 5 - 3 N O I TA M R O F", which is England's 3-5-2 printed reversed, and "3 - 2 - 1- 4 N O I TA M R O F", which is Spain's 4-1-2-3.

#### B. UEFA Women's EURO 2022, 16 teams (UEFA-WTR22): "Key features" shape bullet on each team page

- **Tally [own count]:**
  - 4-3-3 or 4-1-4-1 family: 9 (Belgium is "1-4-3-3 or 1-4-2-3-1").
  - 4-2-3-1: 2.
  - 4-4-2: 3 (Portugal "1-4-4-2 using midfield diamond with switches").
  - 1-3-4-3 primary: 2 (Denmark "1-3-4-3 with fast transitions to 1-5-2-3 defence"; Northern Ireland "1-3-4-3 with wide midfielders dropping into" a 1-5-2-3).
- **Back three or five at some stage:** 5 of 16 (Denmark, England, Finland, Northern Ireland, Sweden).
- **Wiegman interview:** "In 2017 a lot of teams played 4-4-2 and now they play different formations and have different tactics."

#### C. UEFA Women's Champions League 2023/24, 16 group teams (UEFA-UWCL24): SHAPE lines

- **Tally [own count]:** 4-3-3 7, 4-2-3-1 7, back three primary 2 (Benfica "Generally 1-3-4-3 with occasional"; Brann 1-3-4-2-1).
- **Back three or five at some stage:** 5. The three not yet named are:
  - Chelsea: "1-4-2-3-1 with switch to compact 1-5-3-2".
  - PSG: "Initially 1-4-3-3 switching to 1-3-5-2".
  - Häcken: sporadic 1-5-4-1.

#### D. Women's EURO 2025, UWCL 2024/25 and UWCL 2025/26 (UEFA-WTR25, UEFA-UWCL25, UEFA-UWCL26)

No per-team pages; shapes appear only in the match texts.
- **Women's EURO 2025.** A back five or 1-3-5-2 appears for 7 of 16 teams [own reading]: Belgium, Italy, Denmark, Switzerland, Finland, England, and Spain (a back three in build-up: Germany "pushed higher up against Spain’s back three").
  - Out-of-possession shapes named: Denmark "1-4-4-2 mid-block"; Portugal "1-4-3-1-2 diamond structure"; Germany "1-4-5-1 shape without the ball"; Italy "1-5-4-1"; Norway "1-4-2-4".
- **UWCL 2024/25.** Vålerenga used a "1-5-4-1 defensive formation"; Chelsea "a 1-4-4-2 press".
  - Barcelona: "a 3+1 defensive structure, with one of the full-backs inverting to form a back three with a holding midfielder in front."
- **UWCL 2025/26.** The report notes "the continued use of inverted attacking players."

### 1.3 Synthesis (my reading of the figures above)

1. **Starting structures.** A back four is the start for about 3 in 4 teams. A back three or five starts in 23–27 % of World Cup line-ups (men 2022: 27 %; women 2023: 23 %). About half of all teams use one at some stage (EURO 2020 14–15/24, EURO 2024 15/24, WC 2022 18/32). The women's figures are lower: WWC 2023 11/32, Women's EURO 2022 5/16.
2. **In-possession and out-of-possession shapes are different things.** At EURO 2024 a back-three structure made up 52 % of creation-phase shapes, although only 8 of 24 teams *started* with a back three. The extra back threes are built from back fours in one of three ways:
   - a full-back stays low,
   - a full-back inverts,
   - the #6 drops.

   UEFA's shape lines therefore routinely give pairs: "1-4-3-3, defending with 1-4-4-2", "1-4-2-3-1; 1-4-1-4-1 out of possession".
3. **Out of possession,** 4-4-2 or 4-4-1-1 is the most common block. 4-1-4-1/4-5-1 is next, and among back-three teams, 5-4-1 and 5-3-2. Many 4-3-3 and 4-2-3-1 teams defend as a 4-4-2.
4. **Rest-defence structures that are named:**
   - 2+3, the most common at EURO 2024 (UEFA-TR24).
   - 3+2 (FIFA-IFB-CWC25).
   - 3+1 and 4+1 (women's UCL and EURO; UEFA-UWCL25, UEFA-WTR22).
   - "Plus-1" (DFB-AK-RV).
   - 1+3 (Canada, FIFA-WC26).

   These are combinations of the role instructions in §3, not roles of their own.
5. **Diamonds and 4-2-2-2 are niche.** They appear in 1–5 % of uses (EURO 2024: diamond 4-4-2 1/102, 4-2-2-2 4/102; UCL 23/24: Leipzig, Salzburg) and as Mexico's 2026 out-of-possession shape.

---

## 2. Every position interpretation found

The **Evidence** column gives the source key, then a verbatim quote, then the PDF page where there is one.

| ID | Interpretation | Formations seen | In possession | Out of possession | Evidence |
|---|---|---|---|---|---|
| P01 | Goalkeeper | all | Build-up player with the feet | Sweeper behind a high line | FFA-NC p.43: "goalkeepers must be able to play with their feet". FFA-433 s29: "Goalkeeper to act as sweeper." |
| P02 | Centre-back in a back-four pair | 4-3-3, 4-2-3-1, 4-4-2, 4-1-4-1, diamond | Splits wide in build-up; carries into midfield; one of the "2" in a 2+3 rest defence | Marks or covers depending on one or two strikers; leads and organises the line; one steps, one covers; lets a dropping striker go, then closes down | FFA-NC p.44: "know what to do when the opponent plays with one striker or with two, when to close down and mark an opponent and when to drop off and give cover"; "Centre-backs usually have all the other outfield players in front of them"; "should also be able to move into the midfield with the ball at their feet and create a numerical advantage". BDFL-PETER09 p.1: "Als hinterster Feldspieler dirigiert der Innenverteidiger die Abwehrarbeit" [as the deepest outfield player the CB directs the defending]; p.6: "Der Innenverteidiger lässt die Spitze zunächst ziehen und sichert damit weiter den Raum vor dem Tor." [lets the striker go at first and keeps guarding the space in front of goal]. FIFA-WC22-PHYS: "primary focus is to nullify and delay". FIFA-WC26 (France): "One would step into the final third to contest loose balls for his team, while the other defended the space in the middle third." UEFA-TR24 p.60: "two defenders close to the halfway line monitoring the opposition forwards" |
| P03 | Centre-back stepping into midfield (variant of P02) | 4-3-3 / 4-2-3-1 (Man City) | Steps out of the back line into the double pivot | — | UEFA-UCL24 p.15: "Akanji stepped into midfield and formed a double pivot with Rodri" |
| P04 | Central centre-back of a back three or five | 3-4-2-1, 3-4-3, 3-5-2, 5-3-2, 5-4-1 | Deepest distributor, often left free; carries into midfield | Spare man and organiser; covers behind the stepping outside CBs; can follow a dropping striker because others cover behind | FIFA-GI-CHE: "the middle of the three centre-backs, Thiago Silva, is unmarked and free to pick passes and move forward". UEFA-UCL25 p.44 (Villa 4-4-2 → back five or six): "centreback Ezri Konsa was the spare man and chief organiser". BDFL-WM14 p.3: "Aus der Fünferkette konnten Innenverteidiger sich absetzende Spitzen verfolgen, ohne hinter sich Räume zu öffnen." [from a back five, CBs could follow dropping strikers without opening space behind them] |
| P05 | Outside (wide) centre-back of a back three | 3-4-2-1, 3-5-2, 5-3-2, 3-4-3 | Takes the wide lane when the wing-back inverts; overlaps or underlaps; cover and passing option; stays home with the central CB when the other side goes | Steps out of the line to press pockets or wide players, or to follow a runner; covers behind the wing-back | UEFA-TR24 p.83 (Denmark): "Three centre-backs in back line with wide ones free to step up". UEFA-UCL25 p.27: "Wide centre-backs Yann Bisseck (No31) and Alessandro Bastoni (No95) stepped out to apply pressure"; p.28: "wide centre-backs were never afraid of following a player". UEFA-UCL24 p.13: "Inter’s centre-backs and midfielders looked to step out wide as the wing-backs came inside". FIFA-WC26 (Japan, Itō): "By overlapping or underlapping, he became an unexpected player in the final third"; "keeping a centre-midfielder back with their central and right-sided centre-back when Itō ventured forward". DFB-AK-IV p.1: "Die beiden Halbverteidiger besetzen in Ballbesitz die Außenspuren" [the two outside CBs occupy the wide lanes in possession]; "indem sie den Außenverteidiger hinterlaufen" [by overlapping the wing-back]. UEFA-WTR25 p.20 (Belgium): "keeping both wingbacks behind the ball while a centre-back stepped forward to press instead" |
| P06 | Full-back in a back four, classic attacking | 4-3-3, 4-2-3-1, 4-4-2 | Width, overlaps, crosses; free receiver from the goalkeeper; goes high when the winger moves inside | Defends 1v1 against the winger | FIFA-IFB-CWC25: "They are often responsible for creating width for their team in the final third using wide channel runs, overlapping runs". FFA-NC p.43: "the overlapping full-backs that provide the decisive crosses and assists"; "Defensively sometimes they have a direct opponent (winger) which requires strong defensive 1 v 1 skills". FIFA-WC26 (Algeria): "As the wingers, Mahrez and Houssem Aouar, shifted into the inside channels, the full-backs advanced high to stretch the pitch". UEFA-UCL24 p.45 (Lazio): "Good rotations on wings with overlapping, attacking full-backs" |
| P07 | Full-back in a mid or low block | all back-four blocks | — | Narrow next to the CB to protect the inside; tucks in when play is on the far side; stays wide enough to cover a switch or a ball in behind; squeezes in; shows the dribbler outside | FIFA-TSG (WC22 wide areas, Klinsmann): "the full-backs are playing very close to their centre-backs to protect the inside space". BDFL-PETER09 p.4: "Einrücken beim Spiel auf dem anderen Flügel zur Sicherung des Zentrums." [tuck in when play is on the other wing, to secure the centre]. KNVB-R31015: "Knijpen van de backs" [full-backs squeeze in] |
| P08 | Inverted full-back (into midfield) | 4-3-3, 4-2-3-1 → 3-2-5, 2-3-5 | Moves beside or ahead of the #6 in the half-space or centre; forms the "3+2" or "2+3"; counter-presses | Back to the full-back slot, or the #6 covers that slot | FIFA-IFB-CWC25: "Originating from a 1-4-3-3 concept"; Chelsea's "Cucurella, inverted high ahead of Enzo Fernández as an additional attacking midfielder in possession. When they lost the ball, Fernández recovered to the left full-back position"; "player profile matters, with inverted full-backs often being elite counter-pressers"; in build-up they received the ball "4.4 metres narrower than traditional full-backs". DFB-AK-MCI p.1: Zinchenko "rückt in die Zentrumsspur ein und formt eine" [Doppel-Sechs] [moves into the central lane and forms a double six]. UEFA-UCL24 p.38 (Arsenal): see 1.1 D. FIFA-MOFT-RD (Spain 2021): Óscar Gil "came inside to form a box and create the rest defence structure alongside central midfielder" Zubimendi |
| P09 | Low full-back who makes the back three in build-up | 4-x-x → 3-2-5, 3+1, 4+1 | Stays level with or tucked beside the CBs while the other full-back goes | Back in the back four | FIFA-IFB-CWC25: "the right full-back tucked in alongside his centre-backs, so they maintained a 3+2". UEFA-UCL24 p.14 (Dortmund): "full-backs low alongside the centre-backs". UEFA-UWCL25 (Barcelona): "one of the full-backs inverting to form a back three with a holding midfielder in front." UEFA-WTR25 (Italy): "It is common to see one full-back hold their position as the other steps forward". FIFA-WC26-B3: "two centre-backs with one low full-back" |
| P10 | Full-back who becomes a wing-back out of possession | 4-4-2 or 4-x-x defending as a back five (Villa, England, Argentina) | — | Jumps early to the wide player because of cover behind and inside him | UEFA-UCL25 p.37 (Cooper): "For the full-back who's moving to wing-back, this change allows you to be more aggressive and get out to the wide player early". FIFA-WC22-MB: Argentina "utilising both a back four and a back five". UEFA-TR24 p.30: "With England, for the first time in Germany, operating in a back five when out of possession, Saka had a right wing-back role to fill" |
| P11 | Wing-back in a back five | 3-4-3, 3-4-2-1, 3-5-2, 5-3-2, 5-4-1 | Only width provider; joins the front five (3-2-5, "five channels"); available for the switch | In the last line when deep ("flat back five"); the far-side wing-back drops in; decides each time between "fourth man in a 4-4-2" and staying in the five | UEFA-TR20 p.23: "three central defenders and two wingbacks combining in a compact five-man defence"; "In attack the wingbacks pushed forward and stayed wide, always available for the switch of play". FIFA-GI-CHE: "the formation becomes a 3-2-5". UEFA-UCL24 p.14 (Azpilicueta): "Do I go and become the fourth man in a 4-4-2 or do I stay back in a 5-3-2?" FIFA-WC22-PHYS: "The modern full-back/wing-back is now expected to have a dual role". UEFA-WTR25: the far-side wing-back "dropped down into the backline". FIFA-WWC23-TSG: "England push their wing-backs high and wide" |
| P12 | Inverting wing-back (into #8 or #10) | 3-4-2-1, 3-5-2, 3-4-3 | Moves inside as a midfielder or #10 while a winger, wide CB or #8 takes the wide lane; internal runs into the box | As P11 | UEFA-TR24 p.27: "left wing-back Michel Aebischer excelled by coming inside into the No10 position"; p.70: "wing-back João Cancelo would move into midfield". FIFA-GI-CHE: "the Chelsea wing-backs look to move inside to become available for a pass, whereas the wingers move out wide". UEFA-TR20 p.37: wing-backs made "internal runs into scoring positions". UEFA-UCL24 p.38: "a wing-back (Lino) stepping up into midfield" |
| P13 | Single pivot (#6) | 4-3-3 / 4-1-2-3, 4-1-4-1, diamond base | Always available, gives balance; the "+1" of a 2+3 rest defence | Holds and marshals the central space, protects the back four, orchestrates the press; the #8s protect the spaces either side of him | FIFA-WC26 (Casemiro): "Brazil changed to a 1-4-3-3 with Casemiro (5) acting as the pivot/single defensive midfielder. With Guimarães (8) and Paquetá (20) protecting the space to the right and left ahead of him, it allowed him to hold and marshal the central space. Out of possession, he could orchestrate the press"; "protect the back four". UEFA-TR24 p.16: "the perfect No6". FFA-433 s12: "The midfield triangle with the" ["point" (no.6) to the rear]. UEFA-WTR22: "1-4-1-4-1 the default setting with 13 Wälti as" [screen] |
| P14 | #6 who drops into the back line (variant of P13 or P15) | 4-3-3 / 4-2-3-1 → back three | Drops between the CBs or beside the left CB; the full-backs push on | Back to the pivot slot | FIFA-WC26-B3: "By dropping their deep-lying central midfielder, Adrien Rabiot, between the two centre-backs". UEFA-TR24 (Germany): "Kroos dropped to left of centre-backs in build-up as full-backs pushed up". FIFA-IFB-CWC25 (PSG): "Vitinha (17) dropped in alongside his two centre-backs". DFB-AK-MCI p.1: "lässt sich der defensive Mittelfeldspieler Fernandinho in die erste Aufbaulinie zurückfallen" [Fernandinho drops into the first build-up line]. UEFA-UCL24 p.13: Çalhanoğlu "would join the three centre-backs in a back four" |
| P15 | Double pivot (two holding midfielders) | 4-2-3-1, 3-4-2-1, 3-4-3, 3-2-2-3 lower box | One joins the attack, one stays for balance; covers the space in front of the defence against counters | Screens the passing lanes with the CBs; staggered so the far-side 6 covers the near one | FFA-NC p.44: "Their task is to stay centrally and support the back four during BPO as well as feed and support the attack in BP"; "One of the two should always join in to support the attack while the other one stays behind the ball to keep the defensive balance"; "creating a block with the centre-backs and screen the passing lines". BDFL-PETER09 p.5: "Jetzt ist der ballferne Sechser schräg im Rücken des Partners und kann diesen somit sichern." [now the far-side 6 is diagonally behind his partner and can cover him]. UEFA-TR20 p.35: "two midfielders covering the space in front of the defenders for defensive transitions". FIFA-GI-CHE: "The two central midfielders protect the centre-backs defensively". UEFA-WTR22 (Sweden): "two centre-backs and two holding midfielders." |
| P16 | Flat-two central midfielder (4-4-2 out of possession) | 4-4-2, and 4-2-3-1 / 4-3-3 defending as 4-4-2 | As P15 | Pair in the midfield line; when one steps out the wide midfielder tucks in; can be outnumbered by a midfield three | FIFA-WC26 (Brazil): "Casemiro (5) was playing in a flat two, which could be overloaded by their opponent’s midfield three"; "if either centre-midfielder came out to press, the wide midfielder needed to come in narrow". UEFA-UCL25 p.32: in Madrid's 4-4-2 mid-block, "the partnership between midfielders Eduardo Camavinga and Dani Ceballos and the front two" |
| P17 | #8 in a midfield three (back four) | 4-3-3, 4-1-4-1, 4-3-3 → 4-4-2 out of possession | Half-space receiver; runs beyond; moves up to the "upper box" | Jumps out of the line to press (instead of the winger); protects the space either side of the #6 | UEFA-TR24 p.56: Slovakia "pressed from the inside midfield player", "No8s, rather than the wingers, were the players jumping out". FIFA-WC26 (Brazil): Guimarães and Paquetá "protecting the space to the right and left ahead of him". FFA-433 s12: "A "tilting" or rotating triangle" |
| P18 | #8 in a 3-5-2 / 5-3-2 midfield three | 3-5-2, 5-3-2 | Goes wide when the wing-back comes inside; rotates | Has to cover the width in front of a back five; the spaces beside the three are its weakness | UEFA-UCL25 p.27 (Inter): "midfield three working hard to cover the width"; p.9, Southgate: "Paris exploited the space at the side of the midfield three". UEFA-UCL24 p.14: "It is hard for a 5-3-2 team to cover the full width of the pitch against a team building with a back four"; p.13: "midfielder Nicolò Barella going wide" |
| P19 | Upper-box attacking midfielder (two #10s in a 3-2-2-3 box) | 3-2-2-3 built from 4-3-3 / 4-2-3-1 / 3-4-2-1 | Two players between the lines, behind the opposition midfield | Back to the out-of-possession slot (#8, #10 or striker) | UEFA-TR24 p.31: Depay "dropping to join Xavi Simons at the top of their 3-2-2-3 midfield"; p.50: "the box midfield or 1-3-2-2-3 set-up". UEFA-UCL24 p.40 (Bayern): "Kane and Musiala dropped into No10 positions", "forming box with 2 midfielders"; p.15: Bernardo Silva and De Bruyne "operated as a line of two beyond the Madrid central midfield duo" |
| P20 | Central #10 | 4-2-3-1, 4-3-1-2, 3-4-1-2 | Between the lines in the half-space or centre; arrives late in the box; free role | Joins the striker as a front two (4-4-2), or screens the opposition #6 (4-4-1-1) | FIFA-WC26 (England): "Attacking midfielder (10) Jude Bellingham, occupies the half space between opposition midfield and defensive line"; "they often defended in a 1-4-4-2 mid-block, with Kane and Bellingham acting as the front two". FFA-NC p.45: "a midfield position (not a 2nd striker)"; "connect with #6 and #8 to form a compact unit that presses the opponent in the central midfield area". UEFA-UCL25 p.28: Bellingham "edged forward from midfield to form a front two". FIFA-WC26: "Martin Ødegaard’s free role" |
| P21 | Two inside #10s ("2" of 3-4-2-1) | 3-4-2-1, 3-4-3 (narrow front) | Two attackers between the lines in the half-spaces, with the wing-backs giving the width | Usually the wide players of a 5-4-1, or the outer forwards of a 5-2-3 (paraphrase of the shape lines) | UEFA-TR24 p.35: "two No10s in an attacking 3-4-2-1". UEFA-TR20 p.20 (Belgium): "two middle-to-front attackers". UEFA-TR24 p.91 (Poland): "No10 Zieliński got into pockets or made runs in behind" |
| P22 | Diamond roles: tip, sides, base | 4-4-2 diamond / 4-3-1-2 (Leipzig, Salzburg, Mexico, Portugal women) | Tip = #10; sides = #8s; base = #6 | Tip and base lock onto the opposition central midfielders; sides protect the centre and press the full-back | FIFA-WC26 (Mexico): "The players located at the top and bottom of the diamond were responsible for locking on to South Africa’s centre-midfielders"; "The players on either side of the diamond occupied those spaces for two reasons: first, to provide additional protection in the central channel; and second, to press in the wide channel if the ball was played to the full-back in that area". UEFA-WTR25 (Portugal): "1-4-3-1-2 diamond structure" |
| P23 | Wide midfielder in a midfield four | 4-4-2, 4-4-1-1, and 4-2-3-1 defending as 4-4-2, 5-4-1 | Width from the midfield line, or inside | Tucks in when a central midfielder steps out; tracks back with the full-back; holds the mid-block width | FIFA-WC26 (Brazil): "the wide midfielder needed to come in narrow". UEFA-UCL25 p.28: "creating a 1-4-4-2, with Vinícius Júnior on the left and Rodrygo on the right". BDFL-PETER09 p.5: "das Mitarbeiten nach hinten durch die zurückeilenden Außenmittelfeldspieler simuliert" [the wide midfielders sprinting back to help defend] |
| P24 | Winger or wide forward, high and wide (4-3-3 front three) | 4-3-3, 4-2-3-1 in possession | Starts high and wide, pins the opposition full-back, threatens in behind; comes inside for a cross from the far side | Protects his wing with the full-back; presses the back four; or drops into a 4-5-1 | FFA-NC p.45: "Their starting position should always be high and wide."; "together with the full-back, protect and defend their designated wing". FIFA-WC26 (England): "Wingers, (18) Anthony Gordon and (20) Noni Madueke, stay high and wide, stretching the play and/or pinning the opposition full-backs". UEFA-TR24 (Southgate on Spain): "play wingers wide on the touchline and a No9, so you can’t play three against three at the back". FIFA-WC22-PHYS: "two wide forwards who play on either side of a lone centre-forward" |
| P25 | Inside or inverted winger (wide/inside lane decided relative to the full-back or wing-back) | 4-3-3, 4-2-3-1 | Moves into the inside channel when the full-back gives the width, and stays wide when the full-back is inside; acts as an extra man on the last line | Narrow, to win the ball back for counters (Lazio) | UEFA-TR24 (de la Fuente): "the winger has a certain responsibility when the ball is out wide, but that when he’s in the inside spaces, those spaces are occupied by the full-backs". UEFA-UCL25 p.23 (Koundé): "you go outside when he goes inside". DFB-AK-MCI p.1: "Dadurch kann der linke Stürmer Sterling einrücken und als zusätzlicher Überzahl-Spieler an der letzten Linie agieren." [so the left forward Sterling can move inside and act as an extra man on the last line]. UEFA-TR20 p.36: "we called inside-forwards". UEFA-UCL24 p.45: "wingers staying narrow to profit from regains". UEFA-UWCL26: "inverted attacking players" |
| P26 | Wide forward of a 3-4-3 or 5-2-3 (wing-back behind) | 3-4-3, 5-2-3 out of possession, 3-4-2-1 → 5-2-3 | Wide, or swaps lanes with the wing-back | Front-line presser of the outside CB or full-back; the pocket behind him is the weak spot | FIFA-GI-CHE: "whereas the wingers move out wide and operate on the flanks". FIFA-WC26 (Sweden): "Sweden’s out-of-possession shape was 1-5-2-3"; Japan targeted "the pocket of space behind right forward Anthony Elanga and in front of their right wing-back". UEFA-WTR22: "1-3-4-3 with fast transitions to 1-5-2-3 defence" |
| P27 | Lone centre-forward, reference or target | 4-3-3, 4-2-3-1, 4-1-4-1, 5-4-1, 4-5-1 | Plays as high as possible; holds the ball up with his back to goal; target for long balls | Decides when and where the press starts; screens or presses the deepest midfielder (4-5-1) | FFA-NC p.46: "that plays as high as possible"; pressing: "determining when and where to start pressuring the opponent". FIFA-WC22-OV: "more akin to a traditional centre-forward than the" ["false 9"]. FIFA-WC26 (Cabo Verde): "Cabo Verde’s striker is reconnected with Spain’s deepest midfield player". UEFA-TR24 p.31: "Wout Weghorst as a target striker" |
| P28 | Dropping or false 9 (situational behaviour of P27) | 4-3-3, 4-2-3-1 | Drops into midfield to create an overload; swaps with the wingers; the defender must decide whether to follow | As P27 | FIFA-WC26: "Centre-forward, (9) Harry Kane drops down deep into centre midfield to create overload". UEFA-TR24 p.35: "Depay is very good at knowing when to drop"; p.100: "Impact of Güler as false 9, swapping positions with wingers". UEFA-UCL24 p.10: "playing with false nines created confusion". BDFL-WM14 p.3: a "Stürmer, der auch aus der Tiefe kommt" [striker who also comes from deep] versus the "Stoßstürmer" [spearhead striker] |
| P29 | Striker in a pair | 4-4-2, 3-5-2, 5-3-2, 4-3-1-2, 3-4-1-2, and 4-2-3-1 defending as 4-4-2 | Hold-up and combination between the two; one lays off and runs; both attack the cross | The pair presses at halfway or screens centrally; the ball-side forward presses the CB, the other covers the goalkeeper or the pivot; works back to keep the block compact | UEFA-UCL25 p.47 (Southgate): "It's not so often defenders have to play against a pair of strikers in modern football"; p.29: "how hard the front two of Mehdi Taremi and Marcus Thuram worked to help keep the block compact"; p.32: "Mbappé and Vinícius Júnior worked hard, screening centrally". UEFA-TR24 p.57: "dropped off with their two strikers pressing on halfway". FIFA-WC26 (Mexico): "The ball-side forward’s role was to get early, direct pressure on the centre-back receiving the ball, while the second forward actively positioned to get immediate pressure on the goalkeeper". DFB-TE-2ST p.3: "Der Stürmer erhält das Zuspiel, legt auf den Sechser zurück und startet vor das Tor." [the striker receives, lays off to the 6 and runs towards goal] |
| P30 | #10 who becomes a second striker out of possession (4-2-3-1 → 4-4-2 / 4-4-1-1) | 4-2-3-1 | As P20 | As P29, or screening the pivot (4-4-1-1) | UEFA-UCL24: Benfica "1-4-2-3-1; shift to 1-4-4-2 out of possession"; Galatasaray "1-4-2-3-1 when attacking; 1-4-4-1-1/1-4-4-2 out of possession". FIFA-WC26: "Playing as a second striker". FIFA-WC26 (Klinsmann): the partnership "comes more from a 1-4-4-2 formation instead of the 1-4-2-3-1 they want us to believe is their system" |

**Data taxonomy for comparison.** FIFA's own physical data uses eight outfield categories (FIFA-WC22-PHYS): "CB = centre-back, FB/WB = full-back/wing-back, DM = defensive midfielder, CM = central midfielder, AM = attacking midfielder, WM = wide midfielder, WF = wide forward, CF = centre forward". It merges full-back with wing-back, and all centre-backs into one category. The tactical sources above separate them (see §3).

---

## 3. Proposed minimal role set

### 3.1 Design principle the sources force

UEFA and FIFA describe nearly every team with **two shapes**, one in possession and one out of possession. Examples: "1-4-2-3-1; 1-4-1-4-1 out of possession"; "1-4-4-2 (diamond) shape without the ball and in a 1-4-2-3-1 with the ball". The shape change happens because players switch duties between the two phases:
- the #10 becomes a striker;
- the full-back becomes a midfielder;
- the #6 becomes a centre-back.

So I propose:

1. **A role is a set of duties: positional reference, width and depth, rest-defence membership, pressing and marking zone, line leadership, covering relations.** Each role has an in-possession part and an out-of-possession part.
2. **A formation slot names an in-possession role and an out-of-possession role.** By default the two are the same role. A 4-2-3-1 #10 is AM in possession and ST2 out of possession when the team defends in a 4-4-2. A 3-4-2-1 inside #10 is AM (half-space lane) in possession and WM in a 5-4-1.
3. **Movements that change the structure only temporarily stay instructions on the role.** Examples: inverting, dropping, overlapping, staying back, stepping out. They change the slot's in-possession target and its rest-defence membership. They do not create a new role. This follows the project rules on instructable tactics and on situational behaviour without blanket role exemptions.
4. **Team-level instructions are not roles.** These are the in-possession shape (3-2-5, 2-3-5, 3-2-2-3, 4+1), the out-of-possession shape (4-4-2, 4-5-1, 5-4-1, 5-3-2, 5-2-3), the rest-defence structure (2+3, 3+2, 3+1, 4+1, plus-1, 1+3) and the back-four ↔ back-five switch.

### 3.2 The role set (1 goalkeeper + 13 outfield roles)

| Role | Covers (slots and interpretations) | Key distinguishing duties | In possession / out of possession | Instruction variants (stay instructions) | Main sources |
|---|---|---|---|---|---|
| **GK** Goalkeeper | P01 | Defends the goal and the space behind the line; build-up player | IP: extra outfield player in the build-up. OOP: sweeper; height linked to the line | Build-up involvement (short or long); sweeping height | FFA-NC, FFA-433 |
| **CB** Centre-back in a pair | P02, P03; back four in 4-3-3, 4-2-3-1, 4-4-2, 4-1-4-1, diamond | Two players share the central lanes. Mark or cover depending on how many strikers; one steps, one covers; line leader and organiser | IP: split wide to build; one of the "2" or "3" in rest defence. OOP: zonal line; hands over a dropping striker | Step into midfield with or without the ball (Akanji); stay or step in rest defence; aggressiveness against a dropping 9 (follow or hand over) | FFA-NC, BDFL-PETER09, FIFA-WC22-PHYS, FIFA-WC26, UEFA-TR24, UEFA-UCL24 |
| **CBC** Central CB of three or five | P04 | Spare man and organiser of the line. Covers behind the *two* stepping outside CBs. Can follow a dropping 9 because cover exists | IP: deepest distributor, often left free; carries. OOP: last-line leader; covers the depth | Carry into midfield; mark the CF or stay spare | FIFA-GI-CHE, UEFA-UCL25, BDFL-WM14 |
| **CBW** Outside CB of three or five | P05 | Half-space lane between the CBC and the wing-back. **Licence to step out** to press pockets or wide players, or to follow, while the back line stays covered. Covers behind the wing-back | IP: takes the wide lane when the wing-back inverts; overlaps or underlaps; the remaining wide CB stays home with the CBC and a midfielder. OOP: steps out, then recovers | Overlap / underlap / stay home; step-out aggressiveness | UEFA-TR24, UEFA-UCL24, UEFA-UCL25, FIFA-WC26, DFB-AK-IV, UEFA-WTR25 |
| **FB** Full-back in a back four | P06–P10 | Wide lane of a back four, **with a wide player ahead** (W or WM). Width is decided *relative to that player*. Narrow in a block | IP: width and overlap by default. OOP: protects the inside next to the CB; 1v1 against the winger; tucks in on the far side; shows outside | **overlap / underlap / invert** (into the half-space or centre beside the #6, P08) / **stay low** (makes the back three or the 3+1, P09) / **high-wide** extra forward (PSG) / out of possession **drop into a back five** (P10, then plays the WB out-of-possession part) | FIFA-IFB-CWC25, FFA-NC, FIFA-TSG, BDFL-PETER09, KNVB-R31015, DFB-AK-MCI, UEFA-UCL24/25, UEFA-UWCL25, UEFA-WTR25 |
| **WB** Wing-back in a back five | P11, P12 | **Sole width** on its side; no wide player ahead (or a narrow one). Member of a back five **with a CBW behind and inside**. Every situation needs a decision: drop into the five, or step up as a midfielder | IP: in the front five (3-2-5, "five channels"); available for the switch. OOP: in the last line when deep; the far side drops in; jumps early to the wide player because cover exists | **invert** into an #8 or #10 (Aebischer), with a CBW, #8 or wide forward taking the lane; internal runs; stay deep | UEFA-TR20, FIFA-GI-CHE, UEFA-UCL24, UEFA-UCL25, UEFA-TR24, FIFA-WC22-PHYS, UEFA-WTR25, FIFA-WWC23-TSG |
| **DM** Single pivot | P13, P14, diamond base (P22) | **Alone** in the central lane in front of the back line. Protects the back four; marshals the central space; organises the press; the #8s guard the spaces beside him | IP: the always-available "+1" of a 2+3 rest defence. OOP: screens; in a diamond, locks onto the opposition CM | **drop into the back line** (between the CBs, Rabiot; beside the left CB, Kroos); man-mark the opposition #10 or zone; cover the full-back slot behind an inverted FB (Fernández) | FIFA-WC26, UEFA-TR24, FFA-433, FIFA-IFB-CWC25, DFB-AK-MCI, UEFA-WTR22 |
| **CMP** Central midfielder in a pair (double pivot or flat two) | P15, P16, and the CMs of a 4-4-2, 4-2-3-1, 3-4-2-1, 3-4-3, 5-4-1 | **Two players share the centre**. One goes, one stays; staggered cover. When one steps out the WM tucks in. Can be outnumbered by a midfield three | IP: one supports the attack, one holds; covers the space in front of the defence for transitions. OOP: block with the CBs, screens the lanes | Hold / advance (set per partner); drop into the back line; stepping-out aggressiveness | FFA-NC, BDFL-PETER09, UEFA-TR20, FIFA-GI-CHE, FIFA-WC26, UEFA-UCL24, UEFA-WTR22 |
| **CM8** #8 in a midfield three | P17, P18, diamond sides (P22) | Beside or ahead of a single pivot, half-space lane. **First presser out of the midfield line** (instead of the winger). In a 3-5-2 / 5-3-2 has to **cover the width** in front of a back five. Diamond sides press the full-back | IP: half-space receiver; runs beyond; goes wide when the WB inverts; can rise to the upper box. OOP: jump out, or protect the sides of the #6 | Jump-to-press trigger; run beyond / stay; drop into the back line (Barella for a CB); go wide | UEFA-TR24, FIFA-WC26, FFA-433, UEFA-UCL24, UEFA-UCL25 |
| **AM** Attacking midfielder between the lines | P19, P20, P21, diamond tip (P22) | Receives between the opposition midfield and defence. **Lane parameter: centre or half-space.** The sources themselves call the central #10, the two 3-4-2-1 #10s and the box-midfield upper pair all "No10" | IP: in the pockets; late arrival in the box; free role. OOP comes from the slot: ST2 (4-4-2), screening the pivot (4-4-1-1 or diamond tip), WM (5-4-1) or W (5-2-3) | Lane (centre / left or right half-space); free role; drop to build; run in behind | FIFA-WC26, FFA-NC, UEFA-TR24, UEFA-TR20, UEFA-UCL24, UEFA-UCL25 |
| **WM** Wide midfielder in a midfield line | P23; the wide players of 4-4-2 and 4-4-1-1, and of 4-2-3-1, 4-3-3 or 3-4-2-1 when they defend as 4-4-2, 4-5-1 or 5-4-1 | Wide lane **of the midfield line**, inside the full-back's or wing-back's zone. **Covering relation with the CMP or CM8: tucks in when they step out.** Tracks the full-back | IP: width from the midfield line, or inside. OOP: mid-block width; recovery runs | Tuck in / stay wide; track the runner or pass him on | FIFA-WC26, UEFA-UCL25, BDFL-PETER09, UEFA-TR24 |
| **W** Winger / wide forward | P24, P25, P26; front-three wide players of 4-3-3 and 3-4-3, the 4-2-3-1 wide players in possession, outer forwards of a 5-2-3 | Wide lane **of the front line**. **Pins** the opposition full-back or wing-back, high and wide. Wide or inside is decided **relative to the full-back or wing-back** ("you go outside when he goes inside"). Presses from the front line | IP: high and wide by default; comes inside for the far-side cross. OOP: press the back line from outside-in; or stay narrow or high for the counter; or drop to WM in a 4-5-1 | **Wide / inside (inverted)**; press outside-in or curve; stay high for the counter; track back | FFA-NC, FIFA-WC26, UEFA-TR24, UEFA-UCL24, UEFA-UCL25, DFB-AK-MCI, UEFA-TR20, FIFA-WC22-PHYS, UEFA-UWCL26 |
| **CF** Lone centre-forward | P27, P28 | **The only central forward**, so he must both pin the last line and connect play. Pinning or dropping is chosen per situation from his attributes, not as a fixed role. Starts the press | IP: as high as possible; holds the ball up with his back to goal; drops to create an overload when the cues are right. OOP: decides when and where the press starts; screens or presses the deepest midfielder | Pin / come short weighting (the existing `come_short`); target-man play; pressing trigger (who and when) | FFA-NC, FFA-433, FIFA-WC22-OV, FIFA-N9-WC22, FIFA-WC26, UEFA-TR24, UEFA-UCL24, BDFL-WM14 |
| **ST2** Striker in a pair | P29, P30 | **Relational pair: one offers short, one holds the depth**; they combine. Out of possession they press as a pair: the ball-side one goes to the CB, the other covers the goalkeeper or screens the pivot. Works back into the block | IP: hold-up and combination; lay off and run; both attack the cross. OOP: front two of a 4-4-2 or 5-3-2 at the line of pressure | Who offers and who stays deep (set per pair, from attributes); press the CB or screen the pivot (the 4-4-1-1 stagger) | UEFA-UCL25, UEFA-TR24, FIFA-WC26, DFB-TE-2ST, KNVB-R39773 |

### 3.3 How the common shapes map onto the set

The mapping is listed as in-possession role → out-of-possession role. Roles are only named where they change between phases.

- **4-3-3 (4-1-2-3):**
  - IP: GK, CB×2, FB×2, DM, CM8×2, W×2, CF.
  - OOP 4-5-1: W → WM.
  - OOP 4-4-2: one CM8 → ST2, W → WM.
  - 3-2-2-3 build-up: one FB on *stay low*, the other FB on *invert*; or the DM on *drop*.
- **4-2-3-1:**
  - IP: CB×2, FB×2, CMP×2, AM (centre), W×2, CF.
  - OOP 4-4-2: AM → ST2, CF → ST2, W → WM.
  - OOP 4-4-1-1: AM screens the pivot.
- **4-4-2:** CB×2, FB×2, CMP×2, WM×2, ST2×2.
- **Diamond 4-4-2 / 4-3-1-2:** CB×2, FB×2, DM, CM8×2, AM (centre), ST2×2.
- **3-4-2-1:**
  - IP: CBW×2, CBC, WB×2, CMP×2, AM×2 (half-spaces), CF.
  - OOP 5-4-1: AM → WM.
  - OOP 5-2-3: AM → W.
- **3-4-3:** CBW×2, CBC, WB×2, CMP×2, W×2, CF.
- **3-5-2 / 5-3-2:** CBW×2, CBC, WB×2, DM (or CMP pair), CM8×2, ST2×2.
- **4-4-2 → back five (Villa):** FB → WB out of possession; WM → CBW or WB when the team drops into a back six.

### 3.4 Why these merges and splits

**Kept apart:**
- **CB / CBC / CBW.** FIFA's data merges them, but the tactical duties differ in four ways:
  - lane (central vs half-space);
  - licence to step out (CBW "free to step up", "never afraid of following");
  - covering relation (CBC is the "spare man and chief organiser" behind two stepping colleagues, while a back-four pair covers each other one-to-one);
  - in-possession width (CBW "besetzen in Ballbesitz die Außenspuren", overlap or underlap).

  The BDFL-WM14 point that a five lets a CB follow a dropping striker "ohne hinter sich Räume zu öffnen" (without opening space behind) is the covering difference.
- **FB / WB.** Width responsibility differs. The WB is the sole width provider and joins the front five; the FB shares its wing with a W or WM and the lane is relational. Cover differs too: Cooper's "insurance of an extra player not just behind you but inside you" applies to the WB. So does the drop-or-step decision every phase (Azpilicueta). FIFA's merged FB/WB data category reflects physical similarity, not tactical sameness.
- **DM / CMP.** One player versus two in the central lane changes lateral coverage, stepping-out rules ("one goes, one stays"; the far-side 6 covers) and who protects the pivot's sides (the #8s for a DM; the WM tuck-in for a CMP).
- **CMP / CM8.** The CM8 plays beside or ahead of a pivot, is the first presser out of the midfield line, and must cover the width in a 5-3-2. The CMP pair holds the line and covers each other.
- **WM / W.** They sit in different lines: midfield line vs front line (FIFA also separates WM and WF). Their covering relations differ (WM tucks in for the CM; W pins the full-back and presses from the front). A 4-2-3-1 wide player is W in possession and WM out of possession, which the two-role slot handles.
- **CF / ST2.** The pair has an internal relation (offer short / hold depth; ball-side presses / partner covers). A lone 9 must do both, chosen by situation. Southgate notes defenders rarely face "a pair of strikers", which shows the opposition reacts differently too.

**Merged:**
- **Double pivot and flat-two CM → CMP.** A 4-2-3-1 defends as a 4-4-2 (Benfica, Galatasaray, Sevilla, England 2026), so out of possession the two are the same pair. The in-possession hold/advance split is the same rule in FFA-NC and BDFL-PETER09.
- **Central #10, two inside #10s, upper-box pair, diamond tip → AM** with a lane parameter. The sources call all of them "No10" ("two No10s in an attacking 3-4-2-1"; "dropped into No10 positions, forming box"). Their out-of-possession differences come from the slot's out-of-possession role.
- **Diamond sides → CM8.** Their duties (protect the centre, press the full-back) are the #8 duties, with the press-the-full-back trigger as a parameter.
- **Diamond base → DM.**
- **Inverted full-back, low full-back, overlapping full-back → FB with instructions.** Every source describes these as things a full-back *does* in a phase:
  - "inverted high … When they lost the ball, Fernández recovered to the left full-back position";
  - "one full-back hold their position as the other steps forward".

  Out of possession they return to FB duties. FIFA-IFB-CWC25 adds that "player profile matters", which argues for attribute-driven suitability rather than a separate role.
- **#6 dropping into the back line → DM (or CMP) instruction.** It is a build-up movement (Rabiot, Kroos, Vitinha, Fernandinho); the player keeps pivot duties out of possession.
- **Inside or inverted winger → W with the wide/inside instruction.** Lane choice is relational to the full-back (de la Fuente, Koundé, Algeria, Sterling), so it belongs in the W–FB pair logic, not in a separate role.
- **False or dropping 9 → CF situational behaviour.** This matches the existing `come_short` and attribute-profile design (project rule ATT-26).

**Not given a role:** the libero, the sweeper-keeper and the "half-back" (salida lavolpiana). They are covered by CBC, GK and the DM drop instruction. No 2018–2026 source treats them as separate positions.

---

## 4. Gaps

1. **FIFA World Cup 2022 team profiles** (https://documents.fifatrainingcentre.com/FIFA_WC2022/). The SVG text layers give stats and squads but no formation text; shapes are graphics only. Covered instead by the official post-match labels (1.1 A).
2. **No UCL 2022/23 report.** The uefatechnicalreports.com pages for UCL 2022 and 2023 are the same generic page with no PDF link.
   - UCL 2024/25, UWCL 2024/25 and 2025/26, and Women's EURO 2025 have no per-team SHAPE pages, so there are no tallies for them.
3. **EURO 2020 per-team shapes were not extracted** because the team-page layout differs. The report also contradicts itself on back threes (14 vs 15 teams).
4. **World Cup 2026.**
   - The FIFA post-match summary reports were not checked; they would give a third tournament-wide starting-formation count (48 teams).
   - No FIFA TSG final technical report was found. Only articles were read, plus a secondary summary (insideworldfootball) that was not used for claims.
5. **No time-in-shape data.** All counts are line-up labels, SHAPE lines or "teams that used a shape at least once". None gives minutes or phases spent in a shape, except EURO 2024's creation-phase table (times used) and its 2+3 rest-defence statement.
6. **How often each instruction is used is not quantified.** For example, the share of teams inverting a full-back or dropping the #6. The only metrics are FIFA-IFB-CWC25's full-back narrowness (10 m inside the touchline, 4.4 m narrower) and pass-direction shares.
7. **Federation material is thin beyond the 4-3-3 family and back-four play:**
   - no federation text on wing-back duties beyond "fallende Außenspieler" (wide players dropping);
   - no central vs outside CB split;
   - nothing on diamond shuttlers or inside #10s.
8. **Federation material not opened or not found:**
   - The FA Boot Room blogs, including "The pressing role of the wing-back in a back five", were blocked by Imperva (≈1 KB stubs). That title is search snippet only.
   - Old dfb.de trainer pages (back three, BVB U15 pressing in 5-3-2, attacking-midfield position training) return 404.
   - No KNVB per-position task document was found.
   - US Soccer: only 9v9 formation pages are open (the Learning Center needs a login).
   - FFA-433: the deck's provenance is unverified (Squarespace host, 2011, KNVB-style forms inside).
9. **Club analyses vs federation guidance.** The four DFB-Akademie articles (DFB-AK-*) are analyses of club teams, not prescriptions. BDFL-ITK08 is a club (Hertha U17) paper.
10. **Women's data:**
    - No counts of out-of-possession shapes.
    - WWC 2023 TSG pages give no system counts (the tally is from FIFA's line-up labels).
    - Youth tournaments (U-17, U-20) were not tallied. The U-17 women's 2025 TDS PDF has no tactical content.
11. **Secondary analytics** (StatsBomb, Opta and similar) were not used. All figures are from FIFA or UEFA documents or my own counts from them.

---

## 5. Appendix: source keys

**Reused keys:**
- **UEFA-TR24.** Now opened: https://indd.adobe.com/view/publication/0e247454-9a52-47ea-bc78-8607a34a7923/68h3/publication-web-resources/pdf/EURO2024_Technical-Report_DIGITAL_With_Videos_(drudd@twelfthman.co).pdf. Saved as `formations_raw/euro2024_tr.pdf` with `.txt` and `.flow.txt`.
- **FIFA-TSG.** The WC22 "Use of wide areas" page, `formations_raw/wc22_use-of-wide-areas.txt`.
- **FIFA-WC22-MB:** `formations_raw/wc22_controlling-the-game-without-the-ball--t.txt`.
- **FIFA-N9-WC22:** `formations_raw/wc22_the-role-of-the-number-9--more-goals-yet.txt`.
- **FFA-NC.** The full PDF has now been read: https://www.footballaustralia.com.au/sites/ffa/files/2017-09/FFA%20National%20Curriculum_1ma6qrmro1pyq10gzxo5rcn7ld.pdf, saved as `formations_raw/fed/ffa_nc_raw.txt`. The page numbers above are PDF pages; printed page = PDF page − 5.
- **FFA-433:** `formations_raw/fed/ffa_433_deck_raw.txt`, provenance unverified.
- **KNVB-TF:** `formations_raw/fed/knvb_teamfuncties_raw.txt`. It has no position tasks.

**New keys.** Files are under `formations_raw/`.

| Key | Source | URL | Saved file |
|---|---|---|---|
| UEFA-TR20 | UEFA EURO 2020 Technical Report | https://indd.adobe.com/view/publication/6e8ce3d6-ff21-4d31-be78-50651ff1ad3e/hnak/publication-web-resources/pdf/EURO2020_Tech_Report_SCREEN_EN.pdf | `euro2020_tr.*` |
| UEFA-UCL24 | UEFA Champions League 2023/24 Technical Report | https://indd.adobe.com/view/publication/f2775b56-3f10-48b1-af79-55026873e9a5/l7c5/publication-web-resources/pdf/UCL_Tech-Report_2023-24_Digital.pdf | `ucl2324_tr.*` |
| UEFA-UCL25 | UEFA Champions League 2024/25 Technical Report | https://indd.adobe.com/view/publication/5d6d078c-4383-4588-a179-0633807e0b4c/1/publication-web-resources/pdf/UCL_Technical-Report_2025_DIGITAL.pdf | `ucl2425_tr.*` |
| UEFA-WTR22 | UEFA Women's EURO 2022 Technical Report | https://indd.adobe.com/view/publication/e61f91da-a238-4cde-a7d7-4a00f6c738a0/1/publication-web-resources/pdf/WEURO_2022_Technical-Report.pdf | `women/weuro2022_tr.*` |
| UEFA-WTR25 | UEFA Women's EURO 2025 Technical Report | https://indd.adobe.com/view/publication/403923a4-bbbf-4245-9293-1fd6230d4f75/9ny9/publication-web-resources/pdf/WEURO2025_Technical-Report_2025_DIGITAL.pdf | `women/weuro2025_tr.*` |
| UEFA-UWCL24 | UWCL 2023/24 Technical Report | https://indd.adobe.com/view/publication/69794fea-ec5f-4fb9-a93f-9a9b2c0676e2/1/publication-web-resources/pdf/UWCL_Tech-Report_2023-24_Digital.pdf | `women/uwcl2024_tr.*` |
| UEFA-UWCL25 | UWCL 2024/25 Technical Report | https://indd.adobe.com/view/publication/cd173b20-12fd-493c-9fa9-0ee433385e87/1/publication-web-resources/pdf/pdf-uwcl-2025-technical-report.pdf | `women/uwcl2025_tr.*` |
| UEFA-UWCL26 | UWCL 2025/26 Technical Report | https://indd.adobe.com/view/publication/ed8f87f2-61c4-486e-87bb-fc7a88e38944/l8eg/publication-web-resources/pdf/UWCL_Technical-Report_2025-26_DIGITAL_V2.pdf | `women/uwcl2026_tr.*` |
| FIFA-WC22-PMR | FIFA WC 2022 post-match summary reports (64) | https://www.fifatrainingcentre.com/en/fwc2022/post-match-summaries/post-match-summary-reports.php | `wc22_pmsr/`, tally `wc22_pmsr_tally.txt` |
| FIFA-WWC23-PMR | FIFA WWC 2023 post-match summary reports (64) | https://www.fifatrainingcentre.com/en/game/tournaments/fifa-womens-world-cup/2023/match-report-hub/post-match-summary-reports.php | `women/wwc23_pmsr/` |
| FIFA-WWC23-TSG | WWC 2023 TSG pages ("Emerging trends", "Line breaks"); physical part 3 (FIFA-WWC23-PHYS) | https://www.fifatrainingcentre.com/en/game/tournaments/fifa-womens-world-cup/2023/emerging-trends/emerging-trends.php ; …/post-tournament-analysis/line-breaks.php ; …/post-tournament-analysis/physical-analysis/part-3-setting-physical-benchmarks-across-positions.php | `women/wwc23_*.txt` |
| FIFA-WC22-OV | WC22 "Technical-tactical overview" | https://www.fifatrainingcentre.com/en/fwc2022/technical-and-tactical-analysis/technical-tactical-overview.php | `wc22_technical-tactical-overview.txt` |
| FIFA-WC22-PHYS | WC22 physical analysis: "How do various positions cover distances", "Using case studies to reveal the reasons why various tactical roles exert themselves physically…" | https://www.fifatrainingcentre.com/en/fwc2022/physical-analysis/how-do-various-positions-cover-distances.php ; …/physical-analysis/using-case-studies-to-reveal-the-reasons-why-various-tactical-roles-exert-themselves-physically-during-world-cup-matches.php | `wc22_how-do-various-positions-cover-distances.txt`, `wc22_using-case-studies-to-reveal-the-reasons.txt` |
| FIFA-GI-CHE | Game Insights ep. 4 "Chelsea’s principles of attack in a 3-4-3 system" (CWC 2021) | https://www.fifatrainingcentre.com/en/game/game-insights/chelsea_principles_of_attack_in_3_4_3_system.php | `fifa_gi_chelsea343.txt` |
| FIFA-MOFT-RD | Men's Olympic Tournament 2021 "Rest defence". This may already be the "Rest defence" item in FIFA-TSG | https://www.fifatrainingcentre.com/en/game/tournaments/olympic-football-tournament/moft/rest-defence.php | `fifa_moft_rest_defence.txt` |
| FIFA-IFB-CWC25 | CWC 2025: "Emerging positioning of the inverted full-back", "Assessing the influence of inverted full-backs…", "Group stage review", "TSG roundtable" | https://www.fifatrainingcentre.com/en/game/tournaments/fcwc/2025/team-analyses/emerging-positioning-of-the-inverted-full-back.php (and siblings) | `fifa_cwc25_*.txt` |
| FIFA-WC26 | WC 2026 TSG articles (Casemiro tweak, Bellingham, England low build-up, Mexico, Japan v Sweden, France rest defence, Canada v Bosnia, Cabo Verde, Algeria, Ødegaard) | https://www.fifatrainingcentre.com/en/fifa-world-cup-2026/<slug>.php | `wc26_*.txt` |
| FIFA-WC26-B3 | WC 2026 "Building up in a back three and exploiting inside channels" | https://www.fifatrainingcentre.com/en/fifa-world-cup-2026/building-up-in-a-back-three-and-exploiting-inside-channels.php | `fifa_wc26_back_three.txt` |
| BDFL-PETER09 | BDFL ITK 2009, Ralf Peter (DFB-Sportlehrer) | https://www.bdfl.de/images/ITK/2009/ITK09_Peter.pdf | `fed/bdfl_itk09_peter_raw.txt` |
| BDFL-WM14 | BDFL ITK 2014, Stöber/Wormuth (DFB), WC 2014 analysis | https://www.bdfl.de/images/ITK/2014/PDFs/Stoeber_Wormuth.pdf | `fed/bdfl_itk14_stoeber_wormuth.txt` |
| DFB-AK-RV / -MCI / -IV | DFB-Akademie: "Dominanz durch eine effektive Restverteidigung", "Manchester City Spielanalyse", "Innenverteidiger mal anders" | https://www.dfb-akademie.de/dominanz-durch-eine-effektive-restverteidigung/-/id-11010557 ; …/manchester-city-spielanalyse/-/id-11008657 ; …/innenverteidiger-mal-anders/-/id-11008886 | `fed/dfbak_*.txt` |
| DFB-TE-2ST | DFB training unit with two strikers | https://www.dfb.de/fileadmin/_dfbdam/198301-466-b-te.pdf | `fed/dfb_198301_zwei_stuermer.txt` |
| KNVB-R31015 / -R39773 | KNVB Rinus training pages | https://www.knvb.nl/node/31015 ; https://www.knvb.nl/node/39773 | `fed/knvb_node_*.txt` |
| USS-9V9 | US Soccer 9v9 formation page | https://www.ussoccer.com/us-way/player-development/game/formations/u11-u12-9v9 | `fed/uss_9v9.txt` |

**Scripts** in `formations_scripts/`:
- `h2t.py`: HTML to text.
- `svgtext.py`: FlippingBook SVG text.
- `findq.py`: quote check.
- `kw.py`: keyword in context.
- `shapes.py`: UEFA SHAPE lines.
- `wc22_tally.py`: FIFA label tally.
- `pmsr_form.py` and `pmsr_tally.py`: helper agent's women's tally.
- `keyfeat.py` and `kf22.py`: UEFA key-features columns.
