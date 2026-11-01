# How much does team familiarity matter? Effect sizes for the familiarity bench

Research report, 2026-10-31. Written for the familiarity bench of [`../player-brain.md`](../player-brain.md) (sections 7 and 15); the source keys are in [`sources.md`](sources.md) under *Published match data*. "I" and "my" refer to the research session. The downloads and scripts were working copies and are not kept in the repo; every source can be fetched again from its URL, and the Mukherjee reanalysis from the authors' GitHub data. Ten key quotes and the reanalysis were re-checked independently before this copy was made.

## Short answer

Football studies that control for team quality find **small familiarity effects**. Studies that compare a team with itself over time, or separate out the coach's quality, mostly find **no effect at all**. For the bench, where both sides have identical ratings, the most defensible gaps are:

| bench level (role / partners / practice) | settled side's goal difference per match: best (range) | settled side's points per match vs an even match: best (range) | settled win / draw / loss (even match = 37.1 / 25.8 / 37.1 %) | Elo equivalent |
|---|---|---|---|---|
| new manager, new system (0.9 / 0.9 / 0.2) | **+0.04** (0 to +0.10) | +0.03 (0 to +0.07) | 38.0 / 25.8 / 36.2 % | ≈ 6 (0–16) |
| several signings (0.8 / 0.5 / 0.6) | **+0.08** (+0.02 to +0.17) | +0.055 (+0.02 to +0.12) | 38.9 / 25.8 / 35.3 % | ≈ 13 (3–27) |
| just assembled (0.6 / 0.1 / 0.2) | **+0.25** (+0.10 to +0.50) | +0.17 (+0.07 to +0.35) | 43.0 / 25.6 / 31.5 % | ≈ 40 (16–81) |

The points column is how many points per match the settled side gains over what it would expect against an equal team. The other side loses the same amount, so the points gap between the two sides is twice this figure. Conversions use a Poisson model with 2.7 goals per match (§1.4).

Main caveats:
- The only evidence in this range comes from cross-sectional and season-level studies (Franck et al. 2011, Gerrard & Lockett 2018, van Vugt et al. 2008).
- Within-team and causal-style designs find roughly zero: Heuer et al. 2011, Bryson et al. 2021, van Ours & van Tuijl 2016, Le Coz et al. 2025, and my reanalysis of Mukherjee et al. 2019.
- "Just assembled" lies at or beyond the edge of every league dataset, so its figure is an extrapolation.
- No football study isolates role familiarity.

---

## 1. Method

### 1.1 Search and retrieval
- **Sources searched:** Crossref, OpenAlex, Semantic Scholar, Europe PMC, NCBI efetch, arXiv, and general web search for the leads in the brief and for newer work: tracking and event data, pair-level passing, rotation, coach changes, formation changes, and signing adaptation.
- **Storage:** every source used was saved as the original file (PDF/HTML/XML) plus extracted text in the research session (not kept).
- **Blocked sites:** several publishers' sites returned bot challenges (Springer/SpringerOpen, Wiley, HAL, some university repositories). Where that happened I used the abstract from Crossref, OpenAlex or Semantic Scholar and marked the entry **abstract only**.

### 1.2 Quote checking
Every quote in §2 was checked against the saved text with a quote checker (exact match after whitespace normalisation). All passed.

### 1.3 Own computations
- **Mukherjee et al. 2019 reanalysis:** I re-estimated their EPL regression from the authors' public data and Stata do-file (`EPL_seasons_2013-2014.dta` and `EPL_regressions.do` from the repository below; OLS with the do-file's controls).
- **Conversions:** all unit conversions were computed from coefficients typed in from the papers (assumptions in §1.4).

### 1.4 Conversion assumptions
- **Poisson match model:** independent Poisson goals, 2.7 goals per match in total, two otherwise equal sides. A goal-difference edge of *d* per match gives the better side about **0.69·d points per match** above an even match. This matches the 0.6–0.7 rule of thumb in the brief. Examples:
  - *d* = 0.10 → win 39.4 % vs 37.1 %, Elo ≈ 16.
  - *d* = 0.25 → win 43.0 %, Elo ≈ 40.
- **Season studies:** a season-level deficit of *x* points per match (against average opposition) is treated as the other side giving up *x* points per match to the settled side. Season totals are divided by 38 (EPL) or 34 (Bundesliga).
- **Ordered-probit coefficients** (Tamura & Masuda) are scaled by the same model's home-advantage coefficient. That is then anchored to Franck et al.'s Bundesliga home effect of 0.888 goals per match, home versus away for the same team.

### 1.5 How bench levels map onto the variables the studies measured
This mapping is my assumption. Every per-level number in §3 depends on it.

| bench level | squad stability (van Vugt: % of squad already at the club a season earlier; mean 62.6 %, SD 12.8) | average prior league appearances for the club (Franck: mean 61, SD 19; Gerrard: mean 66, SD 25) | coach |
|---|---|---|---|
| settled (0.9/0.9/0.9) | ≈ 80 % (+1.4 SD) | +1 SD (Franck 80, Gerrard 91) | ≥ 2 seasons in post (Gerrard ≈ 100 games) |
| new manager, new system | unchanged | unchanged | new coach, little training time |
| several signings | ≈ 55 % (−0.6 SD) | −1 SD (Franck 42, Gerrard 41), i.e. 4–5 new starters | same coach |
| just assembled | ≈ 30 % (lowest in the sample) | lowest in the sample (Franck 15, Gerrard 20); a truly new XI would be about 0, outside the data | new coach |

### 1.6 Identification labels used in §2
- **correlational:** no adequate quality control.
- **controls for quality:** market value, wages or past performance are included.
- **within-team:** team fixed effects.
- **quasi-causal:** matched controls or reweighting against regression to the mean.
- **causal:** no study reached this level for familiarity.

---

## 2. Studies

### 2.1 Summary table

| key | study | sample | familiarity measure | identification | effect as published (short) | access |
|---|---|---|---|---|---|---|
| FRANCK11 | Franck, Nüesch & Pieper 2011, Schmalenbach Business Review 63:376–392 | Bundesliga 2001/02–2006/07, 3,672 team-matches | average prior league appearances for the current club of the players fielded | controls for quality + home/away team and season FE | goal difference: 0.015·T − 0.112·T²/1000; points: 0.013·T − 0.081·T²/1000 (both p<.10 / <.05) | full text |
| GERRARD18 | Gerrard & Lockett 2018, British Journal of Management 29(1):10–25 | EPL team-seasons 1996/97–2005/06 | appearance-weighted club league appearances; coach games in charge | controls for quality (wages); model 4 adds team FE | Model 2: 0.0368 per log-unit (p<.05); Model 4: member −0.084, coach −0.070, interaction +0.019 | full text (accepted version) |
| VANVUGT08 | van Vugt, Hart & Leader 2008, unpublished manuscript | EPL 1998/99–2006/07, 180 team-seasons | squad stability index | controls for quality (past performance, wealth, contracts, age) | standardised β 0.14 on points (p=.019); goals conceded −0.07 (n.s.) | full text |
| MONTANARI08 | Montanari, Silvestri & Gallo 2008, Journal of Sport Management 22(6):701–716 | Serie A 1994/95–2002/03 | stability and length of team relationships | unknown | positive; weak evidence of an optimum | abstract only |
| MUKHERJEE19 | Mukherjee, Huang, Neidhardt, Uzzi & Contractor 2019, Nature Human Behaviour 3:74–81 | EPL 2013/14, 380 matches (and other sports) | difference in mean prior shared wins per pair | controls for quality + home-team FE (away FE incomplete) | 0.078 goals per unit, p=.001 | full text + authors' data |
| MUKHERJEE19-RE | my reanalysis of the MUKHERJEE19 data | same | same | within-team (full home and away FE) | −0.110 (se 0.045) | own computation |
| LECOZ25 | Le Coz, Carling, Houde & Imbach 2025, Journal of Big Data | 19,721 matches (FIFA-ranking controls, so national teams); 1,602 matches pass-level | shared experience; prior successful passes per pair | controls for quality (ML models) | no independent effect on results; pairs with >10 prior successful passes complete more | abstract only |
| PASARAKONDA25 | Pasarakonda, Maynard, Schmutz, Lüthold & Grote 2025, Group & Organization Management 50(3) | EPL 2006/07–2008/09, 2,280 team-games | months together × minutes per pair | controls (standing, opponent, home, season, game number) | prior familiarity → team pass accuracy γ=.01 (SD .04), p=.78 | full text |
| GRUND16 | Grund 2016, American Behavioral Scientist | EPL, 760 matches, 283,259 passes, 800 careers | network experience (prior shared careers) | correlational + selection controls (dyadic) | pairs who know each other pass more; magnitude not in abstract | abstract only |
| BRANSEN20 | Bransen & Van Haaren 2020, MIT Sloan Sports Analytics Conference paper | 361 seasons, 106,496 matches (Wyscout) | matches played together before the season | predictive ML, no causal claim | chemistry rises fast over first matches, weak beyond ~50 | full text |
| HEUER11 | Heuer, Müller, Rubner, Hagemann & Strauss 2011, PLoS ONE 6(3):e17664 | Bundesliga 1963/64–2008/09 | coach change during the season and in summer | quasi-causal (team-fitness model, matched controls) | in-season +0.030±0.044 goals per match; summer −0.02±0.04 goals per match, −0.3±0.6 points per season | full text |
| BRYSON21 | Bryson, Buraimo, Farnell & Simmons, IZA DP 14104 (2021); published Scottish Journal of Political Economy 71(3):295–322 | France, Germany, Italy, Spain top two divisions 2001/02–2014/15 | coach dismissal/quit, in-season vs off-season | quasi-causal (entropy balancing; ± team FE) | off-season: +0.03 to +0.08 points per match over 20 games; in-season FE −0.035 over 10 games | full text (IZA version) |
| VANOURS16 | van Ours & van Tuijl 2016, Economic Inquiry 54(1):591–604 | Eredivisie, 14 seasons | in-season coach replacement | quasi-causal (counterfactual control group) | no improvement | abstract only |
| GOMEZ21 | Gómez, Lago-Peñas, Gómez, Jimenez & Leicht 2021, Biology of Sport 38(4):603–608 | in-season changes, big-5 leagues 2010/11–2017/18 | coach change | before/after, no control group | short-term gain fading after >10 matches | full text |
| SOUSA24 | Sousa, Clemente, Gouveia, Field & Sarmento 2024, Biology of Sport 41(2):83–94 | systematic review, 24 studies | coach change | review | small or no effects | full text |
| TAMURA15 | Tamura & Masuda 2015, EPJ Data Science 4:9 (arXiv 1503.02484) | J-League; Bundesliga (Kicker and Fussballdaten data) | starting formation differs from the previous match | controls (season strength, home, previous result) | ordered probit −0.011 (n.s.) / −0.034 (p=.019) | full text (arXiv) |
| MEHTA24 | Mehta, Bassek, Garnica-Caparros & Memmert 2024, International Journal of Sports Science & Coaching | 16,720 matches, England/France/Italy/Spain 2010/11–2021/22 | average starting-XI rotation | controls for quality (log market value), mixed model | no universal effect; interacts with market value | abstract only |
| YANG25 | Yang, Zhou, Xu, Yan & Gómez Ruano 2025, Journal of Sports Sciences 43(22) | Chinese Super League 2021, 118 matches, congested schedule | rotation / stability indices | correlational + mediation analysis | −6.4 % win probability per unit of rotation; +6.8 % per 0.1 of stability | abstract only |
| URIBE14 | Uribe & Wang 2014, Collective Intelligence extended abstract (arXiv 1406.7589) | 186 national squads 1990–2010 | club-teammate ties among squad members ("closure") | controls (club quality, league strength, lagged Elo) | closure negative unless brokerage is high | full text (4 pp.) |
| LEHMANN08 | Lehmann & Holmüller 2008, EASM conference abstract | World Cup national teams | club "blocs" | regressions | blocs significant; no magnitudes | abstract only |
| HUCKMAN09 | Huckman, Staats & Upton 2009, Management Science (HBS WP 08-019) | Indian software projects | average prior projects together; role experience | controls | 1 SD familiarity → −18.6 % defects; 1 SD role experience → −14.7 % | full text (working paper) — **outside football** |
| SIEWEKE15 | Sieweke & Zhao 2015, Journal of Organizational Behavior | NBA panel | team familiarity | panel | U-shaped link to coordination errors | abstract only — **outside football** |
| ELBERT25 | Elbert et al. 2025, arXiv 2506.04475 | Age of Empires 2 temporary teams | prior shared games | quasi-random matchmaking | familiarity amplifies the "team player" effect | full text — **outside football** |
| Secondary | CIES report (via Inside World Football), University of Manchester press release, Bocconi news on Grohsjean et al., Analytics FC blog | — | — | descriptive or second-hand | see §2.3 | news/blog |

### 2.2 Study details

**FRANCK11.** Franck, E., Nüesch, S., Pieper, J. (2011). Specific human capital as a source of superior team performance. *Schmalenbach Business Review* 63, 376–392.
- **URL:** https://unifr.ch/tim/en/assets/public/uploads/Publication%20list/2011/sbr_Franck_N%C3%BCesch_Pieper_2011.pdf (full text).
- **Sample:** all Bundesliga matches 2001/02–2006/07, 3,672 team-match observations.
- **Measure (T):** average number of prior league appearances for the current club among the players fielded (mean 61.28, SD 19.15, min 15.29, max 140.29).
- **Outcome:** goal difference and points per match.
- **Controls:** difference in log summed Kicker market values, home, home- and away-team FE, season FE; clustered SEs.
- **Effect as published (Table 2):**
  - Goal difference: T 0.015* (0.008), T²/1000 −0.112* (0.064).
  - Points: T 0.013** (0.006), T²/1000 −0.081* (0.047).
  - Market-value difference 0.400*** (goal difference).
- **Quote (pp. 384–385):** "Because the squared term of our team-specific human capital measure is significantly negative, our results suggest that there is such a concave relation"
- **Quote 2 (pp. 385–386):** "All else equal, a team scores approximately 0.9 goals more in a home match than in an away match."
- **Identification:** controls for quality plus team FE. Strong for this field, but appearances also reflect who the coach rates (selection), so the estimate is probably an upper bound for familiarity alone. Significance is only at 10 % for goal difference.
- **My conversion (curve maxima: goal difference at T = 67, points at T = 80):**
  - Settled (+1 SD) vs −1 SD: 0.049 goals per match (goal-difference equation) or 0.118 points per match (points equation).
  - Settled vs sample minimum: 0.28 goals / 0.34 points per match.
  - Settled vs T = 0 (extrapolated): 0.48 goals / 0.52 points per match.
  - The goal-difference and points equations disagree. The points equation implies ≈1 point per goal, above the usual 0.69.

**GERRARD18.** Gerrard, B., Lockett, A. (2018). Team-specific human capital and performance. *British Journal of Management* 29(1), 10–25. doi:10.1111/1467-8551.12173.
- **URL:** https://wrap.warwick.ac.uk/77078/7/WRAP_BJM%20HC%20and%20team%20performance%20Final.pdf (accepted version, full text).
- **Sample:** EPL team-seasons 1996/97–2005/06. Performance = points / 114.
- **Measures:**
  - Team-member TSHC: appearance-weighted career league appearances for the current club at season start (mean 65.9, SD 25.12, min 20.4).
  - Coach TSHC: games in charge at season start (mean 101, SD 91.8), entered as ln(1+games).
  - Wages: relative wage bill (mean 1.00, SD 0.48).
- **Effect (Table 2):**
  - Model 2 (OLS): ln member TSHC 0.0368** (0.0178); wages 0.2027***.
  - Model 4 (team FE, the authors' preferred model): member −0.0840*, coach −0.0699 (n.s.), member×coach +0.0192*.
  - Table 4: a 2 SD rise gives wages +9.1 points per season and the TSHC terms a net +2.8.
- **Quote (p. 20):** "The impact of a two standard deviation increase in wage costs is 9.1 points"
- **Quote 2 (p. 21):** "The positive impact of the cross product is 16.4 points, more than twice the impact of general HC."
- **Quote 3 (p. 13):** "Ln(1 + Team Manager TSHC) to avoid the problem of Team Manager TSHC having a zero value when a new head coach has been appointed prior to the start of the new season"
- **Quote 4 (p. 19):** "We found no evidence that this was the case, the Team Member TSHC quadratic term being highly insignificant."
- **Identification:**
  - Model 2 controls for quality only.
  - Model 4 is within-team, but coach tenure inside a team tracks success: good coaches stay, bad ones are sacked. The coach terms are therefore confounded.
  - The IV model (model 8) is not significant.
- **My conversion:**
  - Model 2: settled (+1 SD) vs −1 SD = 3.4 points per season = 0.089 points per match (≈ 0.13 goals); settled vs sample minimum = 6.3 points = 0.165 points per match (≈ 0.24 goals).
  - Model 2 wage-bill equivalents: ≈ 15 % and 27 % of the league-average wage bill.
  - Model 4: new coach vs a 101-game coach = −5.6 points per season (−0.15 points per match) at average squad tenure, −8.8 (−0.23) at +1 SD.
  - Model 4 also predicts that a just-assembled side with a new coach does *better* than a settled one. The interaction form produces this oddity, which shows how fragile the model is.

**VANVUGT08.** van Vugt, M., Hart, C. M., Leader, T. (2008). *Does stability foster team performance? A European football (soccer) inquiry.* Unpublished manuscript.
- **URL:** https://www.professormarkvanvugt.com/images/files/DoesStabilityFosterTeamPerformance.pdf (full text).
- **Sample:** EPL 1998/99–2006/07, 180 team-seasons. Small extra samples: Championship and Serie A 2000/01.
- **Measure:** share of the squad already at the club at the start of the previous season. Mean 62.6 %, SD 12.8, range 30.3–92.0. Season points: mean 52.0, SD 15.6.
- **Effect:**
  - With all controls (past performance, contract length, wealth, age): points β = 0.14 (p = .019); goals conceded β = −0.07 (n.s.); goals scored β = 0.16.
  - Coach change: 47.63 vs 53.69 points raw; controlled β = 0.11 (p = .042).
- **Quote (p. 10):** "the mean player stability index for the teams across the nine seasons was 62.6%"
- **Quote 2 (p. 15):** "number of points (R2 = .52; team stability: β = 0.14, t(174) = 2.37, p = .019"
- **Quote 3 (p. 15):** "the number of goals conceded in the new model (R2 = .44; team stability: β = -0.07, t(174) = -1.10, p = .274"
- **Quote 4 (p. 16):** "collected fewer points (M = 47.63, SD =13.53 vs. M = 53.69, SD = 16.01; t(178) = 2.35, p = .020)"
- **Quote 5 (p. 17):** "manager stability: β = 0.11, t(172) = 2.05, p = .042"
- **Quote 6 (p. 4):** "of all the 529 players who started in the English Premier League in 2006/7 177 (33.5%) were first-timers at their club"
- **Quote 7 (p. 19, Serie A):** "Yet controlling for average contract length reduced the relationship between stability and performance"
- **Quote 8 (p. 16):** "However, teams with managerial change did not concede more goals"
- **Identification:** controls for quality, season level. Not peer reviewed.
- **My conversion:** 1 SD of stability = 2.2 points per season = 0.057 points per match. Under my mapping:
  - Several signings (2 SD) → 0.11 points per match (≈ 0.16 goals).
  - Just assembled (3.9 SD, extrapolated linearly to the sample minimum) → 0.22 points per match (≈ 0.33 goals).
  - Coach change (dummy; β × SD_y / SD_x = 0.11 × 15.6 / 0.444) → ≈ 3.9 points per season ≈ 0.10 points per match. This is correlational: clubs change coach after bad seasons.

**MONTANARI08.** Montanari, F., Silvestri, G., Gallo, E. (2008). Team performance between change and stability: the case of the Italian "Serie A". *Journal of Sport Management* 22(6), 701–716.
- **Access:** abstract via Crossref.
- **Sample:** all players of the 35 Serie A teams, 1994/95–2002/03.
- **Quote:** "team stability and longevity of team relationships have a positive impact on performance"
- **Quote 2:** "there is weak evidence that team stability is beneficial up to a critical point, after which it might be detrimental"
- **Notes:** no magnitudes in the abstract. Identification unknown. Not used numerically.

**MUKHERJEE19** and my reanalysis **MUKHERJEE19-RE.** Mukherjee, S., Huang, Y., Neidhardt, J., Uzzi, B., Contractor, N. (2019). Prior shared success predicts victory in team competitions. *Nature Human Behaviour* 3, 74–81. doi:10.1038/s41562-018-0460-y.
- **URL:** https://www.ncbi.nlm.nih.gov/pmc/articles/PMC6446935 (author manuscript, full text). Data and code: https://github.com/smukherjee0305/Skills_Shared_Success_Sports
- **Sample:** EPL 2013/14, 380 matches.
- **Measure (δS):** difference between the two teams in the mean number of prior shared wins per player pair (2005/06–2012/13).
- **Controls:** prior-season goals/shots/assists per game, home-team and away-team dummies.
- **Effect (Table 2, EPL):** δS 0.078 goals per unit, p = 0.001; R² 28.4 % → 31 %.
- **Quote:** "The prior shared success of a team had a positive and significant impact on the difference of team scores (d.f. = 379, P = 0.001, effect size statistic = 0.078, 95% CI = 0.033 to 0.122)"
- **Quote 2:** "the inclusion of prior shared success resulted in a modest increase of R2 from 28.4% to 31%"
- **Reanalysis (my computation, from the authors' `EPL_seasons_2013-2014.dta`):**
  - The authors' do-file leaves out the away dummies for Manchester City, Manchester United and Norwich, which pools them as the reference.
  - With that dummy set I get δS 0.068 (se 0.033), close to the published figure.
  - With full home and away team FE, δS = **−0.110 (se 0.045)**; −0.079 without skill controls.
  - Using all shared matches instead of shared wins: +0.023 with the paper's dummies vs −0.044 (se 0.018) with full FE.
  - SD of δS is 6.13, so at face value 1 SD = 0.48 goals. Most of that variation is between teams: team means range from 0.02 (Cardiff) to 15.8 (Manchester United), which tracks quality.
- **Identification:** the published result is controls-for-quality with an incomplete set of fixed effects. With complete FE the EPL result does not hold. **Not used as positive evidence.**

**LECOZ25.** Le Coz, S., Carling, C., Houde, T., Imbach, F. (2025). Reassessing the impact of shared experience in football. *Journal of Big Data*. doi:10.1186/s40537-025-01239-x.
- **Access:** abstract only, via Semantic Scholar. The open-access PDF on Springer was blocked.
- **Sample:** 19,721 matches at match level. The abstract cites FIFA-ranking controls, so these are national teams; the competition list is not verified. 1,602 matches at pass level.
- **Quote:** "conventional metrics like FIFA rankings and team strength overshadowed shared experience, which showed no independent influence on results"
- **Quote 2:** "player pairs with over 10 successful passes achieved higher completion rates"
- **Quote 3:** "both short-term (20-day) and long-term (12-month) shared experience windows were predictive, with no added benefit from exponential decay weighting"
- **Identification:** controls for quality (machine-learning prediction). The most recent and largest study, and the only one using pass-level data at scale. **No effect on results** once strength is known. Effect sizes for passing are not given in the abstract.

**PASARAKONDA25.** Pasarakonda, S., Maynard, T., Schmutz, J. B., Lüthold, P., Grote, G. (2025). How team familiarity mitigates negative consequences of team composition disruptions: an analysis of Premier League teams. *Group & Organization Management* 50(3). doi:10.1177/10596011231193176.
- **Access:** full text via Europe PMC (PMC11999323).
- **Sample:** EPL 2006/07–2008/09, 2,280 team-games (Opta + Transfermarkt).
- **Measure:** prior and current-season familiarity (months or minutes played together per pair).
- **Coordination:** team pass accuracy.
- **Effect (Table 3, Model 2):** prior familiarity → pass accuracy γ = .01 (SD .04), p = .78. For scale, team standing γ = 3.54.
- **Other results:** current-season familiarity has a U-shaped link to pass accuracy. Prior familiarity softens the coordination loss after a red card.
- **Quote:** "There was no U-shaped relationship between prior team familiarity and team coordination."
- **Quote 2:** "we found a significant interaction between loss disruption duration and prior team familiarity (γ = .01, p = .036)"
- **Quote 3:** "having established either a little or a lot of current team familiarity, within one EPL season, is better for team coordination than a moderate level of current team familiarity"
- **Quote 4:** "teams with high prior team familiarity might even be able to increase coordination"
- **Identification:** controls for quality (standing).

**GRUND16.** Grund, T. U. (2016). The relational value of network experience in teams. *American Behavioral Scientist*. doi:10.1177/0002764216643133.
- **Access:** abstract via OpenAlex.
- **Quote:** "Soccer teams exhibit a higher passing rate when players know each other from before."
- **Quote 2:** "Controlling for selection effects, two players are more likely to pass the ball between each other when they know each other more."
- **Quote 3:** "network experience has no effect on the decentralization of team play"
- **Notes:** magnitudes are not in the abstract; the full text was not retrieved.

**BRANSEN20.** Bransen, L., Van Haaren, J. (2020). Player chemistry: striving for a perfectly balanced soccer team. MIT Sloan Sports Analytics Conference.
- **URL:** https://arxiv.org/pdf/2003.01712 (full text).
- **Sample:** 361 seasons, 106,496 matches. Pair chemistry is joint VAEP per 90 minutes together. A CatBoost model predicts chemistry for new pairs.
- **Quote (§4.3):** "their mutual chemistry can quickly increase after playing a few matches together. Beyond the mark of 50 matches together (i.e., slightly more than a season in most competitions), this effect is much weaker."
- **Identification:** feature importance in a predictive model. Gives the shape over time, not a magnitude in goals.

**HEUER11.** Heuer, A., Müller, C., Rubner, O., Hagemann, N., Strauss, B. (2011). Usefulness of dismissing and changing the coach in professional soccer. *PLoS ONE* 6(3), e17664. doi:10.1371/journal.pone.0017664.
- **Access:** full text.
- **Sample:** Bundesliga 1963/64–2008/09.
- **In-season dismissals vs control teams matched on goal difference:** change in goal difference after dismissal = +0.030 ± 0.044 goals per match.
- **Summer coach change (relative to expected fitness):** −0.3 ± 0.6 points per season; −0.02 ± 0.04 goals per match.
- **Quote:** "dismissing the coach within the season has basically no effect on the subsequent performance of a team"
- **Quote 2:** "a possible systematic effect of a CC is less than one point per season, i.e. totally negligible."
- **Quote 3:** "the average goal difference per match) we obtain −0.02±0.04"
- **Quote 4** (the authors explicitly allow for a disruption cost): "negative (e.g. due to corrosion of well-established team structures)"
- **Identification:** quasi-causal; the fitness model handles regression to the mean.
- **My conversion:** −0.009 ± 0.018 points per match. Treating ± as one standard error, the 95 % interval for a summer change runs from about −0.10 to +0.06 goals per match.

**BRYSON21.** Bryson, A., Buraimo, B., Farnell, A., Simmons, R. *Special ones? The effect of head coaches on football team performance.* IZA DP 14104 (2021). Published in *Scottish Journal of Political Economy* 71(3), 295–322.
- **URL:** https://docs.iza.org/dp14104.pdf (full text).
- **Sample:** top two divisions of France, Germany, Italy and Spain, 2001/02–2014/15. 273 teams, 769 coaches.
- **Method:** entropy balancing on pre-change form.
- **Effect (Table 7, mean points per match over the next n games after a dismissal; no FE / team FE):**

| | next 1 game | next 5 | next 10 | next 20 |
|---|---|---|---|---|
| between seasons | +0.104 (0.075) / +0.102 (0.083) | +0.122*** / +0.084** | +0.096*** / +0.047 | +0.077*** / +0.029 |
| within season | −0.022 (0.049) / −0.004 (0.054) | −0.012 / −0.025 | −0.022 / −0.035** | +0.002 / −0.008 |

- **Quote (p. 24):** "If we instead focus only on coaching changes during the offseason (following a dismissal), team performance improves across the whole follow up period (apart from the first game of the season)."
- **Quote 2 (p. 24):** "Focusing on mid-season dismissals, changing Head Coach appears to make little difference to team performance, while the fixed effects model suggests that team performance declines somewhat over games 7-15."
- **Quote 3 (p. 26):** "we find teams can experience positive returns after a dismissal of between 0.04-0.1 points per game"
- **Identification:** quasi-causal for the change as a whole. It cannot separate the new coach's quality from the cost of learning a new system.

**VANOURS16.** van Ours, J. C., van Tuijl, M. A. (2016). In-season head-coach dismissals and the performance of professional football teams. *Economic Inquiry* 54(1), 591–604.
- **Access:** abstract via IDEAS (dataset on Zenodo).
- **Quote:** "we conclude that replacement of head-coaches does not improve team performance"
- **Identification:** quasi-causal (counterfactual control group). Reports no disruption cost either.

**GOMEZ21.** Gómez, M. A., Lago-Peñas, C., Gómez, M. T., Jimenez, S., Leicht, A. S. (2021). Impact of elite soccer coaching change on team performance according to coach- and club-related variables. *Biology of Sport* 38(4), 603–608. doi:10.5114/biolsport.2021.101600.
- **Access:** full text.
- **Quote:** "team’s short-term performance was improved significantly with a change to a new coach with this impact declining in the longer term (> 10 matches)"
- **Identification:** before/after only, with no control group, so the bounce matches regression to the mean. Used only for timing.

**SOUSA24.** Sousa, H., Clemente, F. M., Gouveia, É. R., Field, A., Sarmento, H. (2024). Effects of changing the head coach on soccer team's performance: a systematic review. *Biology of Sport* 41(2), 83–94. doi:10.5114/biolsport.2024.131816.
- **Access:** full text (PMC10955743).
- **Quote:** "Dismissing the HC within the season had basically no effect on the subsequent performance of a team."
- **Notes:** no evidence specific to formations or playing systems.

**TAMURA15.** Tamura, K., Masuda, N. (2015). Win-stay lose-shift strategy in formation changes in football. *EPJ Data Science* 4, 9. doi:10.1140/epjds/s13688-015-0045-1.
- **URL:** https://arxiv.org/pdf/1503.02484 (full text).
- **Method:** ordered probit of match result on "starting formation differs from the previous match", controlling for season strength, home and previous result.
- **Effect:**
  - Table 4, Bundesliga (Kicker): −0.011 (se 0.016, p = .51); home 0.420.
  - Table 4, J-League: +0.112 (p = .075).
  - Table 10, Bundesliga (Fussballdaten): −0.034 (se 0.014, p = .019).
- **Quote:** "formation changes did not statistically improve the results of succeeding matches"
- **Quote 2:** "when the Fussballdaten data were used, formation changes significantly decreased the probability of winning"
- **My conversion (scaled by the home coefficient × 0.888 goals):** −0.02 to −0.07 goals in the match after a formation change.
- **Identification:** controls. Formation changes are endogenous: they follow losses and may be forced by injuries or suspensions.

**MEHTA24.** Mehta, S., Bassek, M., Garnica-Caparros, M., Memmert, D. (2024). "Chop and change": examining the occurrence of squad rotation and its effect on team performance in top European football leagues. *International Journal of Sports Science & Coaching*. doi:10.1177/17479541241274438.
- **Access:** abstract via OpenAlex.
- **Quote:** "universally deploying squad rotation may not yield higher points over a season, but also shows how the effect of rotations on team success keep increasing as the market value of teams increases"
- **Notes:** no magnitudes in the abstract.

**YANG25.** Yang, Zhou, Xu, Yan, Gómez Ruano (2025). The negative impact of squad rotation on football match outcomes: mediating roles of passing and shooting performance. *Journal of Sports Sciences* 43(22). doi:10.1080/02640414.2025.2561345.
- **Access:** abstract via OpenAlex.
- **Quote:** "each increase in Rotation3 reduced the probability of winning by 6.4%, with passing performance explaining 11.9% of this effect"
- **Quote 2:** "A 0.1 increase in Stability1 was associated with a 6.8% higher probability of winning"
- **Notes:** small sample (118 matches, congested fixtures only). The units of "Rotation3" are not defined in the abstract. Correlational. Not used numerically.

**URIBE14.** Uribe, J., Wang, D. (2014). Learning from others, together: brokerage, closure and team performance. Collective Intelligence 2014 (arXiv 1406.7589). Full text (4 pp.).
- **Quote:** "closure reduces performance until brokerage reaches relatively high levels (about the 75th percentile in our sample)"
- **Notes:** national squads, Elo outcome, controls for club quality and lagged Elo. No magnitudes given. Suggests that more club-teammate ties in a national squad are not simply better.

**LEHMANN08.** Lehmann, E. E., Holmüller, J. (2008). The success of national football teams: superstars or team effects? EASM conference abstract.
- **Quote:** "Also team attributes (legionnaires, bloc formation and average age) significantly shape the success of football teams."
- **Notes:** no magnitudes.

**HUCKMAN09 (outside football).** Huckman, R. S., Staats, B. R., Upton, D. M. (2009). Team familiarity, role experience, and performance: evidence from Indian software services. *Management Science*. doi:10.1287/mnsc.1080.0921.
- **Access:** HBS working paper 08-019, full text.
- **Quote:** "An increase of one standard deviation in overall team familiarity results in an 18.6% decrease in expected defects."
- **Quote 2:** "An increase of one standard deviation in overall average role experience yields a 14.7% decrease in expected defects."
- **Quote 3 (abstract):** "the level of team familiarity (i.e., the average number of times that each member has worked with every other member of the team) has a significant positive effect on performance"
- **Why it matters here:** the only source found that measures **role experience** and **partner familiarity** side by side. Their effects are of similar size.

**SIEWEKE15 (outside football).** Sieweke, J., Zhao, B. (2015). The impact of team familiarity and team leader experience on team coordination errors: a panel analysis of professional basketball teams. *Journal of Organizational Behavior*. doi:10.1002/job.1993.
- **Access:** abstract only.
- **Quote:** "Our findings support the U‐shaped relationship between team familiarity and TCEs"

**ELBERT25 (outside football).** Elbert, von Schenk, Kosse, Klockmann, Stein, Flath (2025). What drives team success? arXiv 2506.04475 (Age of Empires 2).
- **Quote:** "this effect is amplified by team familiarity—teams with prior shared experiences benefit more from the presence of team players"

*Not retrieved:*
- Berman, Down & Hill 2002 (NBA, Academy of Management Journal; doi 10.2307/3069282): the title was confirmed through Crossref, but no figures were read.
- Grohsjean, Piezunka & Mickeler (Strategic Management Journal): only the Bocconi news item below was read.

### 2.3 Secondary and grey sources (labelled, not used for estimates)
- **CIES Football Observatory report, via Inside World Football**. Second-hand and descriptive:
  - Quote: "the better ranked teams recruited on average 38.5% of their squad during the year, while this percentage was 43.8% for the lower ranked clubs"
- **University of Manchester press release** (student project with Opta data; methods not published):
  - Quote: "take 6 matches to adjust if they transfer from Spain, or 18 matches if they transfer from France or the Netherlands"
  - Quote 2: "there is no adjustment period if they transfer to England from Spain, but they might take nine matches if they come from the German Bundesliga"
- **Bocconi news on Grohsjean, Piezunka & Mickeler** (Strategic Management Journal): club teammates who faced each other at the 2018 World Cup passed to each other less in the next club season (difference-in-differences). Second-hand:
  - Quote: "passed the ball to each other around 11% less often"
- **Analytics FC blog** (summary of BRYSON21 by members of the same group):
  - Quote: "changing a coach during the off-season results in fairly sizeable positive effects of up to 0.13 points per game"
- **American Soccer Analysis 2018 (MLS roster consistency):** descriptive charts of week-to-week line-up changes against table position. No usable effect size was extracted.

---

## 3. Synthesis per bench level

All figures are for the settled side against the other side, ratings equal. "Points per match" means the settled side's gain over an even match; the other side loses the same amount.

### 3.1 New manager, new system (practice 0.9 → 0.2; role and partners unchanged)
**What the evidence measures:** a coach change with the squad largely intact. This bundles three things:
1. the cost of learning a new system (what the bench models);
2. the difference in coach quality (zero in the bench);
3. any "new coach" stimulus.

| source | estimate converted to settled-side advantage | identification |
|---|---|---|
| HEUER11, summer change | +0.02 ± 0.04 goals per match; +0.009 ± 0.018 points per match | quasi-causal |
| BRYSON21, off-season, first 1–20 games | new coach does **better**: −0.03 to −0.12 points per match for the settled side | quasi-causal |
| BRYSON21, in-season, team FE, next 5 / 10 / 20 games | +0.025 / +0.035 / +0.008 points per match (≈ +0.04 / +0.05 / +0.01 goals) | quasi-causal |
| VANOURS16, HEUER11 in-season | ≈ 0 | quasi-causal |
| TAMURA15, one formation change | +0.02 to +0.07 goals in the next match | controls |
| VANVUGT08, coach change | ≈ +0.10 points per match (≈ +0.15 goals) | correlational/controls |
| GERRARD18 Model 4, new coach vs 101-game coach | +0.15 to +0.23 points per match | within-team, confounded by coach survival |

- **Best estimate:** +0.04 goals per match (+0.03 points per match). **Range:** 0 to +0.10 goals (0 to +0.07 points).
- **Why:** the designs that deal with regression to the mean put the whole coach-change effect near zero. In-season changes, where the new coach has no preseason, show at most a dip of about 0.03–0.04 points per match in games 5–15. The larger correlational figures are confounded with team decline.
- **Caveat:** a pure learning cost could hide behind an equally large gain from coach quality or stimulus. The data rule out a **net** cost bigger than about 0.1 goals per match, not a large learning cost that is offset. Real coach changes also seldom change the system as completely as practice 0.2 implies.

### 3.2 Several signings (role 0.8, partners 0.5, practice 0.6)
**What the evidence measures:** squad stability or average club appearances, i.e. 4–5 new starters. These measures bundle partner familiarity, role familiarity and practice; the studies cannot separate them.

| source | settled (+1 SD) vs −1 SD | identification |
|---|---|---|
| VANVUGT08 | +0.11 points per match (≈ +0.16 goals) | controls for quality |
| FRANCK11, goal-difference equation | +0.05 goals (≈ +0.034 points) | controls + FE |
| FRANCK11, points equation | +0.12 points per match (≈ +0.17 goals) | controls + FE |
| GERRARD18 Model 2 | +0.089 points per match (≈ +0.13 goals); ≈ 15 % of the league-average wage bill | controls for quality |
| GERRARD18 Model 4 (same coach) | +0.012 points per match | within-team |
| MUKHERJEE19-RE, LECOZ25 | ≈ 0 (no positive effect) | within-team / controls |
| MEHTA24 (rotation) | no universal effect | controls |

- **Best estimate:** +0.08 goals per match (+0.055 points per match; ≈ 13 Elo; win 38.9 % vs 35.3 %). **Range:** +0.02 to +0.17 goals.
- **Why:** this is the middle of the controlled cross-sectional estimates (0.05–0.17 goals), pulled down because:
  1. appearance and stability measures partly capture quality the controls miss (players kept and selected are the ones the coach rates);
  2. the within-team designs and the national-team study find nothing.
- **Skill equivalents (Gerrard Model 2, rough):** 0.08 goals ≈ 2.1 points per season ≈ 9 % of the league-average wage bill.

### 3.3 Just assembled (role 0.6, partners 0.1, practice 0.2)
**What the evidence measures:** the least stable teams in league samples (stability ≈ 30 %; ≈ 15–20 prior club appearances). These sit at the edge of the data. A truly new XI with no shared history is outside it.

| source | settled vs least-familiar | identification |
|---|---|---|
| VANVUGT08 (linear, 3.9 SD) | +0.22 points per match (≈ +0.33 goals) | controls for quality |
| FRANCK11 vs sample minimum | +0.28 goals / +0.34 points | controls + FE |
| FRANCK11 vs zero appearances (extrapolated) | +0.48 goals / +0.52 points | extrapolation |
| GERRARD18 Model 2 vs sample minimum | +0.165 points per match (≈ +0.24 goals); ≈ 27 % of wage bill | controls for quality |
| practice part (from §3.1) | +0.04 goals | quasi-causal |
| LECOZ25 (national teams) | no independent effect on results | controls |

- **Best estimate:** +0.25 goals per match (+0.17 points per match; ≈ 40 Elo; win 43.0 % vs 31.5 %). **Range:** +0.10 to +0.50 goals (+0.07 to +0.35 points).
- **Why:** the three quality-controlled studies agree on about 0.24–0.33 goals at the sample minimum. I have not added more for the extrapolation to zero history: concavity (FRANCK11) and the null within-team and national-team results argue against it.
- **Caveat:** national teams are all fairly "assembled" and LECOZ25 finds no effect there. This suggests the gap among low-familiarity sides is small. It does not tell us how a national-team-like side would fare against a settled club side.

### 3.4 Separating partners, practice and role
- **Practice (coach/system):** the only component with its own evidence (§3.1). About 0 to 0.1 goals per match at the start of a new system, near zero after a preseason.
- **Partners:** stability and appearance measures are dominated by partner familiarity but also include role and practice for new players. Pair-level studies (GRUND16, LECOZ25 micro, BRANSEN20) show that partner familiarity changes passing between pairs, but none converts this into goals. If partners carry most of the level B and C gaps: about 0.06 goals per match from partners 0.9 → 0.5 and about 0.2 goals from 0.9 → 0.1. This split is my inference.
- **Role:** **no football study isolates role familiarity.** Outside football, HUCKMAN09 finds role experience (−14.7 % defects per SD) about as important as partner familiarity (−18.6 % per SD). A bench split of roughly equal weight for role and partners per unit of shortfall would be consistent with that, but it rests on one software-industry study.

---

## 4. Time course

| what | evidence | plateau / timing |
|---|---|---|
| pair familiarity (partners) | BRANSEN20: rises fast over the first matches together, much weaker beyond ~50 matches. FRANCK11: goal-difference benefit peaks at ≈ 67 prior club appearances (points: 80). GERRARD18 Model 2: log-shaped, each doubling of appearances ≈ +2.9 points per season | ≈ 50 matches (≈ 1–1.5 seasons) to near plateau; peak ≈ 2 seasons of regular starts |
| how long familiarity lasts | LECOZ25: 20-day and 12-month windows both predictive; down-weighting older history adds nothing | familiarity does not visibly fade within a year |
| within-season build-up | PASARAKONDA25: current-season familiarity has a U-shaped link to pass accuracy | not monotonic within a season |
| new signing (individual) | Manchester press release only: forwards 6–18 matches, defenders 0–9 matches after a league move | ≈ 0–18 matches; mixes league adaptation with team adaptation |
| new coach / system | BRYSON21: off-season appointments show no dip, even in game 1; in-season appointments show a small dip in games 7–15 (team FE) that is gone by game 20. HEUER11/VANOURS16: no effect. GOMEZ21: short-term bounce fades after ~10 matches (regression to the mean). GERRARD18 Model 4: squad tenure only helps once the coach has ≈ 78–79 games in charge (model-dependent) | preseason (~6 weeks) appears enough; in-season, any cost seems to vanish within ≈ 15–20 matches |

**Suggested functional form (my inference, for the bench):**
- **Partner familiarity:** f(n) = 1 − exp(−n/τ), with n = matches played together and τ ≈ 20. This gives 0.39 after 10 matches, 0.71 after 25, 0.92 after 50 and 0.96 after 67, in line with BRANSEN20 and FRANCK11.
- **Practice:** a much shorter time constant. Roughly 4–6 weeks of training or 5–10 matches, since off-season appointments show no early dip.

---

## 5. Mechanism numbers

- **How often familiar pairs pass to each other:**
  - GRUND16: pairs who know each other pass to each other more, after controlling for selection; the abstract gives no magnitude.
  - Grohsjean et al. (second-hand via Bocconi): a social shock (facing each other at a World Cup) cut pair passing by about 11 % in the next season. So pair passing frequency responds to the relationship at roughly a 10 % scale.
  - BRANSEN20: pair chemistry (joint VAEP per 90 minutes) increases with matches played together; no magnitude given.
- **Pass completion:**
  - LECOZ25 (1,602 matches): pairs with more than 10 prior successful passes have higher completion rates, especially for low, ground and high passes; the abstract gives no percentages.
  - PASARAKONDA25: team-level pass accuracy is unrelated to prior familiarity (γ = .01, p = .78), compared with γ = 3.54 for team standing.
  - Taken together, any completion gain is concentrated in particular pairs and pass types and is small at team level.
  - YANG25: passing explains 9.8–11.9 % of the rotation–result link (Chinese Super League, small sample).
- **Goals conceded by new back lines:** no tracking or event-data study was found.
  - VANVUGT08: stability predicts goals scored (β 0.16) but not goals conceded (β −0.07, n.s.) once controls are added.
  - VANVUGT08: teams that changed coach did not concede more.
  - So the weak correlational evidence points to the attack, not the defence, as where familiarity shows.
- **Coordination errors:**
  - SIEWEKE15 (NBA): U-shaped link between familiarity and coordination errors; abstract only.
  - HUCKMAN09 (software): −18.6 % defects per SD of familiarity.
  - PASARAKONDA25: prior familiarity softens the drop in pass accuracy after a red card (γ = .01, p = .036).
- **Interaction with player type:** ELBERT25 (Age of Empires) finds familiarity amplifies the value of "team players". This is outside football, but it fits the plan to read coordination from player attributes (player-brain section 7).

---

## 6. Gaps

1. **No causal football estimate of partner familiarity on results.** The positive estimates are season- or match-level regressions with quality controls. They are probably inflated by selection: retained and frequently fielded players are the ones the coach rates. The within-team and national-team analyses find no effect.
2. **No football study of role familiarity.** Nothing found compares a player in a familiar role with the same player in an unfamiliar one, measured in goals.
3. **No tracking-data study of new back lines.** Nothing found on offside-line coordination, gaps between defenders, or the effect of new centre-back pairs on chances conceded.
4. **"Just assembled" is outside the league data.** The estimate is an extrapolation. Expansion sides, newly founded clubs or post-bankruptcy squads would be natural tests; none were found.
5. **Abstract-only sources:**
   - LECOZ25: the most relevant recent study. Springer blocked the PDF; the full text would give the pass-completion magnitudes.
   - GRUND16, MONTANARI08, MEHTA24, YANG25, SIEWEKE15 and VANOURS16 were also read as abstracts only.
   - Berman et al. 2002 (NBA) was not read at all.
6. **New-signing adaptation:** only a student project press release. No peer-reviewed curve for a player's output after a transfer within the same league was found.
7. **New system vs coach quality:** no study separates the cost of learning a system from coach quality. The coach-change literature bounds only the net effect.
8. **Formation changes:** one study (TAMURA15), with endogenous changes and mixed results across two datasets.
