# SBWG — Design Document

> Working title TBD. A real-time grand-strategy game of tribes, cities, and belief in a
> fictional pre-Islamic desert world.

**Status:** Living document. This is the source of truth for design intent. Update it when
decisions change; don't re-derive decisions in conversation. Sections marked **[OPEN]** are
unresolved; sections marked **[LOCKED]** are decided and should only change deliberately.

---

## 1. Pitch

You are not a general moving units. You are a leader steering a desert people through a single
sitting — from a wandering band to a city-state or a nomad confederation — by setting direction,
incentivising semi-autonomous followers, managing fractious internal factions, and shaping the
**culture** that both empowers and constrains everything you do. Four players contend over a
shared map dense with minor tribes and villages, fighting mostly through proxy, trade, and belief
rather than open war.

## 2. Genre framing **[LOCKED]**

This is a **real-time grand-strategy / political simulation with indirect control** — *not* a
conventional RTS. No unit micro, no APM race, no base-building-as-combat. Players select
**places** (tiles/regions) and **set intent**; semi-autonomous followers decide how to get there
and what to do. Orders can be misunderstood, ignored, or contravened.

**Touchstones:** Majesty (indirect control via incentives — the closest ancestor), Crusader Kings
(factions, culture, legitimacy, revolt), Northgard (real-time, region-claim, low-micro 4X-lite),
Patrician/Anno (trade interdependency), Civ city-states / Total War minor factions (the minor-power
field).

## 3. Design pillars

1. **Indirect control.** You influence, you don't puppet. Uncertainty is a feature, but it must
   always be *legible and mitigable* — never opaque RNG punishment.
2. **Culture is the soul.** Every system feeds into and reads from culture. Culture empowers
   with-the-grain play and taxes against-the-grain play.
3. **Politics over battle.** All-out war is rare and costly; skirmishes, proxy conflict, trade,
   and belief are the everyday. Autarky is a viable but all-or-nothing gamble.
4. **Interdependence.** The dense field of minor powers is the medium of play, not set dressing.
5. **One sitting, full arc.** A 60–90 minute session takes a society from small and simple to a
   distinct, fully-formed personality.

---

## 4. Locked decisions (spec snapshot)

| Decision | Choice |
|---|---|
| Session model | **Session-based**, target **60–90 minutes** |
| Map | **Square/region grid**, fixed, no infinite scroll; two scales (fine tiles + regions) |
| Presentation | **"Ants at zoom-out"** — cosmetic swarm rendered from cohort counts + real movers (leaders/caravans/parties); buildings 4–6 tiles, people ~1 tile, continuous movement |
| Phases | **Lateral** — two axes (settlement mode × scale), not a tech ladder |
| Nomadism | **Fully playable end-state** (tribute + trade + raid), not just "level 1" |
| Players | **4 max**, plus a dense field of minor semi-autonomous villages/tribes |
| Minor powers | **The board & victory denominator**; 3 standing flavours → control ladder (Aligned→Tributary→Subjugated→Absorbed); reactive agents; governance-capped |
| Player setup | **Asymmetric** — different starting cultures & leaders |
| Support | **4 faction tracks, no separate overall bar** (Military, Economic, Religious, Populace) |
| Culture | **4 axes** (Martial↔Conciliatory, Open↔Insular, Devout↔Worldly, Hierarchical↔Egalitarian); compressed clock; passive diffusion cut, active ops kept |
| Religion | Reframed as **influence** (contested per-region value), not binary conversion |
| Influence | **Double-duty substrate** for both religious spread and cultural-ops spread |
| Labour & slavery | **Core production input**; assignable cohorts; free/waged · debt-bonded · chattel; slavery = cheap-now / leveraged-risk |
| Setting | **Fictionalised-but-grounded** — pre-Islamic Arabian/Bedouin flavour, no direct real-world references |
| Succession | **Event-driven** (assassination/battle/gambit), not an age clock |

---

## 5. Setting & tone **[LOCKED]**

Fictional desert world with the *flavour* of pre-Islamic Arabia: caravan trade, tribal
confederations, oasis settlements, poetry and patronage, a religious landscape in flux. **Invent
cultures, peoples, deities, and places** inspired by the period — no direct depiction of real
ethnic or religious groups. This buys creative freedom and avoids reducing living traditions to
mechanics (e.g. the "narcotics to intoxicate a populace" op lands on a fictional people, not a
real one).

Period inspirations to mine (not copy): caravan city-states, south-Arabian kingdoms, the
muʿallaqāt poetic tradition, polytheism syncretising under pressure from emerging monotheisms.

---

## 6. Game structure

### 6.1 Session shape **[LOCKED]**
- **60–90 minutes.** Economy/culture tuned around this.
- **Victory conditions** (reach threshold across the map and **hold N minutes**):
  - **Economic** — control X% of trade.
  - **Military** — subjugate X% of post-band instances.
  - **Religious/Influence** — dominate the influence layer over X% of regions/population.
  - **Hegemonic** — combined thresholds.
- **Score fallback:** if no one triggers a win by the time limit, highest composite score wins.
  (Prevents stalls.)

### 6.2 Spatial model **[LOCKED]**
**Two scales on one fixed map** (no infinite scroll):
- **Tile substrate (fine):** terrain, resources, and building footprints snap to a **square grid**
  (8-way for pathfinding/placement). A tile is roughly **person-sized**; **buildings span 4–6
  tiles**; a settlement is *hundreds* of tiles of buildings, open ground, and crowd. The map is
  large in tile-count.
- **Regions (strategic):** a region is a **cluster of tiles** (a settlement footprint / territory)
  and is the unit the strategic systems operate on — claiming, control state, influence (§11),
  support, and win-condition accounting (§8).

People (the masses) **move continuously over the grid, not one-per-cell** — the difference between
an anthill and a chessboard (see §17.4 for the sim/render split that makes the swarm cheap).

- **Water is terrain-bound** (oasis/river/well tiles) → positional, brittle water logistics, and
  across a sprawling settlement the haul is long and visible.
- **Settled** players hold fixed territory; **nomads** claim looser grazing ranges and move camps.
- **Indirect control fits:** designate a place/region as intent; followers route themselves there.
- **Zoom spectrum:** zoomed out → strategic board (control/influence/support overlays); zoomed in →
  the teeming detail. The range itself is a feature.

### 6.3 Phases — two axes **[LOCKED]**
Phases are **lateral**, not a ladder. Two independent axes:

1. **Settlement mode (fork):** Nomadic ↔ Settled. Switchable at a cost. The core Bedouin-vs-settled
   tension. Nomad is a legitimate path to victory, not a starter state.
2. **Scale (development arc, with tradeoffs — not strictly-better):**
   - Settled: hamlet → village → city → state
   - Nomad: band → clan → confederation

You still *grow* across a session (regions, population, faction complexity), but climbing scale is
a choice with tradeoffs. A nomad confederation is a valid endgame opposite a city-state.

---

## 7. The four players **[LOCKED: asymmetric]**
Each starts with a **distinct culture and leader(s)**, seeding different axis positions, favoured
activities, and starting frictions. Asymmetry is the replay engine. Candidate archetypes (Raiding
Horde / Caravan League / The Faithful / Merchant Princes) are sketched in §10. **[OPEN]** final
roster and whether authored vs. drafted/randomised at lobby.

## 8. Minor powers — the board **[LOCKED: central]**
A dense field of semi-autonomous villages and tribes is the **primary arena** and the **denominator
for victory** — "subjugate X%", "control X% of trade", "influence X%" are all measured against the
minor field (§6.1), so their count and spread *is* the scoreboard. They deliver three pillars at
once: **proxy conflict** (players fight *over* minors, not each other → war stays rare),
**interdependence** (minors hold the diverse goods, mercenaries, water and trade you can't easily
self-supply → autarky is the costly all-or-nothing path), and **targets for every activity**.

**No-micro guardrail:** you never manage minors directly. You point *activities* at the map (trade
routes, preachers, an army parked nearby, a raid) and minors **react**; standing accrues passively
from what you're already doing. You garden/predate an ecosystem — you don't click 30 village menus.
Holding this line keeps minors from becoming the micro the design rejects.

**What a minor is:** a board piece, not a player — a settlement/tribe on one+ tiles with its own
**culture** (same 4 axes, fixed-ish), a **specialty** (type), a **disposition** per player, and a
**control state**. They don't pursue victory and don't climb phases.

### 8.1 Standing — three flavours **[LOCKED]**
Each player accrues three kinds of standing with each minor; whichever **dominates** decides which
relationship is reachable (this is why the four archetypes play the same board differently):

| Flavour | Generated by | Leads to | Archetype |
|---|---|---|---|
| **Commercial** | trade routes, gifts, goods diversity | Alignment (trade partner) | Caravan League |
| **Cultural/Religious** (the §11 influence layer) | preachers, shared culture, conversion ops | Conversion → Absorption | The Faithful |
| **Coercive** | nearby army, raids, intimidation | Tributary (protection) | Raiding Horde |

(Merchant Princes mix Commercial + Coercive → tributary trade hubs.)

### 8.2 Control ladder **[LOCKED]**
How vassalage/tribute is represented: a soft→hard ladder, each rung reached by different leverage
and held at a different cost.

| State | Reached by | You get | Cost / fragility |
|---|---|---|---|
| **Independent** | — | nothing (courtable) | — |
| **Aligned** *(soft)* | dominant Commercial or Cultural standing | favourable trade, mercenaries-for-hire, leans your way | none, but reversible — rivals out-bid you |
| **Tributary** *(hard pact)* | dominant Coercive standing, or formalising an Aligned tie | tribute (goods/gold/**labour**), follows your lead, keeps its culture | must stay credibly strong; rebels if over-squeezed or out-influenced |
| **Subjugated** *(military)* | conquest/occupation | heavy extraction incl. **captives→slaves** (§14) | garrison upkeep; restive; rival Cultural standing → revolt; tanks standing with culturally-linked minors |
| **Absorbed** *(permanent)* | deep sustained Cultural dominance + time (Open/Devout) | minor integrates into your populace/territory; ceases to be a minor | slow, hardest — but permanent |

### 8.3 Contestation — the proxy war **[LOCKED]**
Rivals flip your minors without touching your capital: **out-trade/out-convert** your Aligned ones;
**incite revolt** in Tributary/Subjugated minors by raising Cultural standing (same lever as
inciting a slave revolt, §14); **raid the caravans** feeding an Aligned tie. Constant low-grade
contest over the minor field *is* the everyday conflict ("skirmishes common, all-out war rare").

### 8.4 Behaviour — reactive disposition agents **[LOCKED]**
Minors are **not** strategic AIs. Each has three drives — **Security, Prosperity, Identity** — and
reacts *legibly* (§15): raid them → they fear & hate you and seek a protector (**possibly your
rival**); trade → goodwill; convert → identity drifts. Threatened minors take protection from
whoever offers safety; prosperous hubs play players off each other; over-squeezed tributaries seek a
liberator. **Over-aggression has blowback** — raid too much and minors flee to rivals and
culturally-linked minors turn on you; self-balances the Martial build and delivers "violence against
a well-loved culture is unpopular". *(Emergent events — relievable droughts, charismatic chiefs,
resource finds — are a later layer.)*

### 8.5 Specialisation **[LOCKED]**
Types make players want *different* minors → competition + trade:
**Oasis/well** (water; can choke a rival, §13) · **Caravan/market hub** (trade & goods diversity) ·
**Warrior tribe** (mercenaries + captive raids) · **Shrine/holy site** (religious influence,
pilgrimage) · **Craft village** (specific goods) · **Herder band** (mobile, livestock/food, hard to
pin).

### 8.6 Couplings **[LOCKED]**
- **Nomad/settled asymmetry:** nomads excel at Coercive standing (raid, tribute) but can't Absorb
  and struggle to garrison → the **slave supply side**; settled players Subjugate/Align/Absorb → the
  **demand side** (§14).
- **Governance ceiling:** the number of Tributary/Subjugated minors you can hold is capped by
  **scale + military**; overreach → cascading revolts. Doubles as the **anti-snowball brake** for
  4-player balance.

**Slice first:** ~a handful of minors, 3 types (oasis/market/warrior), the 3 standing flavours,
**Aligned + Subjugated** states, reactive fear/goodwill + revolt, basic contest. Defer Tributary
pact terms, Absorption, emergent events, and the full type roster.

---

## 9. Activity systems
Available across phases, weighted by phase and culture:
- **Military** — mercenary work, raids, defence, law & order.
- **Economic** — trade, crafts/production, resource extraction, entertainment.
- **Religious** — healing, spiritual services, influence/spread, rallying.
- **Leadership** — reward/punish behaviours, edicts, special buildings, policy & resource
  allocation.

Production, extraction, crafts, and construction are all gated by **labour** (§14). Each activity
both *consumes* culture/support state and *feeds back* into it (see §10–11).

## 10. Culture system **[LOCKED: central, compressed; 4 axes]**
Culture is the soul mechanic. **Four axes** — three mapping to the win paths, one cross-cutting
governance axis — small enough to read at a glance:

| Axis | With-the-grain (cheap / bonused) | Against-the-grain (costs support + disobedience) | Touches |
|---|---|---|---|
| **Martial ↔ Conciliatory** | *Martial:* raids/war/subjugation cheap & potent, captive→slave synergy, Military support easy. *Conciliatory:* diplomacy, trade-binding, amnesty cheap & support-boosting; low war-weariness | Martial paying for peace/mercy; Conciliatory raising armies or declaring war | Military activity & support, Military win |
| **Open ↔ Insular** | *Open:* high trade volume & **goods diversity** (raises support ceiling), absorbs foreign cohorts/freed slaves & ideas. *Insular:* resists rival influence ops, homogeneity/unity bonus | Open = **porous to enemy influence ops** & dilution; Insular = capped goods diversity, weak trade, isolation tax | Economic activity & support, Economic win, influence-susceptibility (§11) |
| **Devout ↔ Worldly** | *Devout:* religious ops (influence spread, healing, rallying) cheap & potent, clergy content, omens/blessings. *Worldly:* free of taboos, economic/military pragmatism cheap & flexible | Devout breaking sacred custom / trading forbidden goods; Worldly neglecting clergy | Religious activity & support, Religious/Influence win |
| **Hierarchical ↔ Egalitarian** | *Hierarchical:* chattel slavery cheap & accepted, **edicts land hard**, elites cheap to placate. *Egalitarian:* manumission build, **Populace loyal & revolt-resistant**, consensus legitimacy | Hierarchical = restive Populace, nastier succession; Egalitarian = weaker edicts, no cheap coerced labour, grumbling elites | Leadership activity, Populace support, labour/slavery (§14), succession (§16) |

**Hierarchical↔Egalitarian** is the cross-cutting governance axis (and the slavery axis, §14).
Built-in tensions fall out for free: the economic player *must* go Open and is therefore culturally
porous; the slaver wants Hierarchical and gets a restive Populace. You can't be strong everywhere.

**How it behaves:**
- **Continuous sliders with thresholds.** An action flips from with-grain to against-grain when you
  cross a threshold — legible gating plus real commitment.
- Actions nudge axes by **chunky, visible** amounts, and most actions move **two axes at once**
  (enslave captives → Martial + Hierarchical), so builds **cluster into archetypes** rather than
  à-la-carte. ~30 culture-affecting decisions/session → a distinct personality by endgame: a
  *character arc played in one sitting*.
- **Inertia:** reversing a deep conviction takes sustained against-grain action, each costing
  support. Cultures don't pivot on a dime.
- "The grain" = current axis positions, which drives the §15 disobedience chance.
- Starting position seeded by player archetype (§7) + first few choices.
- **Always-visible read:** a 4-spoke radar / four labelled sliders; hover shows what each pole makes
  cheap or dear *right now*.

**Natural clusters** (candidate starting cultures for §7):

| Archetype | Lean | Wins via |
|---|---|---|
| **Raiding Horde** | Martial · Hierarchical · Insular | Military (conquest + slave economy), nomad-leaning |
| **Caravan League** | Conciliatory · Open · Egalitarian · Worldly | Economic (free-labour trade), settled-leaning |
| **The Faithful** | Devout · Hierarchical | Religious/Influence (missionary or fortress-faith) |
| **Merchant Princes** | Open · Hierarchical · Worldly | Economic/Hegemonic (trade wealth + mercenaries) |

**Cultural ops (kept):** active, fundable ops (send preachers to spread pacifism, merchants with
narcotics to a rival) — resolve in minutes, pure agency; effectiveness gated by the target's
Open↔Insular position.
**Cut for session play:** slow *passive* cross-border diffusion. (Could return as an optional
long-game mode.)

Pole names above are mechanical/provisional; in-world flavour names can come later.

## 11. Influence system **[LOCKED: shared substrate]**
A single **influence layer** — a contested per-region/per-population value — underlies **both**
religious spread and cultural-ops spread. Not a binary flip; regions are tugged between competing
sources. The religious victory is dominance of this layer over X% of the map.

## 12. Support & factions **[LOCKED]**
**Four support tracks, no overall bar.** Each unhappy faction fails *differently* — turning
support management into flavourful political triage rather than "keep one bar green":

| Track | Faction (by phase) | Failure mode when low |
|---|---|---|
| **Military** | families' warriors / war chief / war lord / generals | mutiny, coup |
| **Economic** | merchant / guild leader / barons | strikes, capital flight, embargo |
| **Religious** | preacher / religious leader / religious institutions | unrest, legitimacy curses |
| **Populace** | general population (all phases) | revolt, desertion, defection to a rival's influence |

The masses (Populace) are tracked separately from the elites — revolts come from the masses.
Pleasing one faction at another's expense is the central, unavoidable tension.

## 13. Resources
- **Water** — primary survival resource; terrain-bound; transport from source to population centre
  is key and brittle (§6.2).
- **Food** — subsistence is free at small scale; growth needs farming and/or steady supply.
- **Goods** — *diversity* of incoming goods raises a support ceiling (e.g. no goods → support caps
  at ~50%). **[OPEN]** exact goods list and ceiling curve.
- **Labour** — the universal production input; sourcing it (free/bonded/slave) is a core strategic
  posture (§14).
- **Support** — see §12 (the four faction tracks).

## 14. Labour & slavery **[LOCKED]**
Labour is the **universal production input**: a tile's potential (water, crops, stone, crafts,
construction, caravans) yields nothing until labour works it. *How you source labour* is a core
strategic posture with cascading cultural and political consequences.

**Model:** assignable **cohorts** (work-gangs assigned to a tile/operation) — not a faceless pool,
not individuals. Fits the ECS and the no-micro rule, and lets traits (skill, culture-of-origin,
compliance) and emergent stories ride along. Three **statuses**, with movement **both ways**:

- **Free / waged** — *expensive, stable, productive.* Cost: **gold** (wages) + sensitivity to
  **Populace support**. Higher productivity/head; can do **skilled** work (high-value crafts,
  entertainment, religious services); flexible to reassign.
- **Debt-bonded** — *the reversible middle rung.* Free people who fall into bondage in hard times
  (water/food/economic crisis); redeemable back to free, or sink to chattel. A downward spiral that
  punishes economic mismanagement and a font of emergent drama.
- **Chattel / slave** — *cheap now, leveraged risk later.* Cost: **military** (guards) + **food** +
  **acquisition** + a standing **revolt risk** + **cultural/diplomatic lock-in**. Lower
  productivity/head, poor at skilled work (good for extraction, hauling, construction),
  foot-dragging/sabotage variance.

**Central tension:** slavery *looks* cheaper (low gold) but is **secretly leveraged** — you carry
tail risk, cultural lock-in, and enmity. A pure slave economy is a **glass cannon**: out-produces
everyone, then shatters. It must **never be strictly optimal** — a mixed economy is the stable
default, full abolition a distinct ideological build.

**Acquisition** (wires slavery into other loops):
- **War captives** from raiding/subjugating minors and players → a **war → captives → cheap
  production → more war** flywheel feeding the military victory path.
- **Trade** at slave markets (a commodity; lets non-raiders buy in).
- **Debt-bondage** from your own impoverished Populace.
- **Tribute** from vassals/minors.

**Settlement-mode asymmetry:**
- **Nomads** = the **supply side** — best captors (raids), can't run big guarded workforces; they
  sell bodies at settled markets.
- **Settled** = the **demand side** — scale coerced labour into quarries/fields/monuments, bear the
  full guard cost and revolt risk.

**Failure mode & attack vector** (no 5th support bar — slavery is a *pressure*, not a faction):
- Slaves feed a discrete **revolt risk**, mitigated by military allocation, spiked by high
  slave-ratio, thin guard-coverage, or war pulling guards to the front.
- **Inciting a rival's slave revolt** is an influence/espionage op (§10–11) — the slaver build's
  named weakness, so its economic dominance has a counter.
- Heavy slavery can lift *free* Populace mood short-term (cheap goods, no hard labour) — a sugar
  high masking the time-bomb.

**Culture lock-in:** slavery is gated by **Hierarchical↔Egalitarian** (which largely *is* the
slavery axis), modulated by **Pious↔Secular** (clergy bless or condemn). With-the-grain
(Hierarchical): cheap, accepted. Against-the-grain (Egalitarian turning to mass slavery): heavy
support penalties + disobedience risk. Path-dependent — earlier choices make it cheap or ruinous.

**Two-way door / abolition build:** **manumission** boosts Populace/Religious support, shifts
culture Egalitarian, and freed people join your Populace **carrying their culture-of-origin**
(feeding the influence/cultural-mix layer §11). This makes **abolition a viable, asymmetric victory
path**: the egalitarian/pious player *liberates & converts* toward an **influence/religious** win,
opposite the hierarchical/militarist *enslave & extract* **economic/military** win — exactly the
asymmetry the four starting cultures want (§7).

**Tone:** slavery was integral to the period's caravan economies and warfare, so it belongs and
adds weight. Keep it **mechanically consequential and morally textured** (victims have culture and
identity, costs are real, liberation is viable) — not gamified-neutral or edgy. The fictional
setting means no real group is cast as enslaver or enslaved (§5).

## 15. Indirect control & uncertainty **[LOCKED: legible]**
You set intent and incentives; followers act with autonomy. Orders can be ignored, misunderstood,
or contravened — but the player must always be able to *read the odds and influence them*
(low support + culturally-misaligned order ⇒ high disobedience chance, with visible levers). Model
the Majesty/Rimworld principle: agent "irrationality" is predictable once you learn the rules.

## 16. Succession **[LOCKED: event-driven]**
No aging clock (doesn't fit a session). Leaders die from **events** — assassination, battle, a
faction's gambit — triggering a **succession crisis** (power vacuum, factions grab the seat) as a
mid-game spike of tension.

---

## 17. Technical direction

### 17.1 Stack
- **Rust + Bevy** (ECS suits a sim of many population groups, leaders, caravans, regions).
  Bevy **0.18.1**, edition 2024. Workspace of three crates (§17.2); first scaffold builds & tests green.

### 17.2 Architecture **[LOCKED: intent]**
- **Split a headless simulation crate from the Bevy client.** The sim is the authoritative game
  logic, runs without rendering, and is unit-testable in isolation. Single most important structural
  decision — keeps multiplayer and testing tractable.
- Workspace (implemented): `sim` (headless authoritative core, `SimPlugin`) + `client` (Bevy
  rendering/input) + `protocol` (Bevy-free serialisable wire types). Systems stay decoupled via
  Bevy **messages** — `IncomingCommand` (world→sim) and `OutgoingEvent` (sim→world) — which is the
  seam the network layer will later occupy.
- **Module convention: one concern, one module, one `Plugin`.** `sim` = `map` (world state) +
  `entity` (components + tick behaviour) + `setup` (worldgen) + `messages`; `client` = `world`
  (camera/terrain/territory) + `entities` (sprites/swarm/caravans) + `player_input`. Top-level
  `SimPlugin`/`ClientPlugin` just compose the sub-plugins.

### 17.3 Netcode **[LOCKED: approach]**
- **Server-authoritative simulation with state replication.** *Not* lockstep determinism (a trap
  in Rust/Bevy: float, system ordering, hashmap iteration order).
- The design's **low input rate and coarse simulation are a genuine advantage**: run an
  authoritative tick at a slow rate (≈5–20 Hz strategic) and replicate. Latency-tolerant,
  cheat-resistant, far simpler.
- Evaluate `bevy_replicon` (replication) and `lightyear` (batteries-included). **[OPEN]** pick one.

### 17.4 Presentation & scale **[LOCKED: cosmetic swarm + real movers]**
The "ants at a zoom-out" look (dense, numerous — *not* AoE's sparse-symbolic cities) comes from
**simulating aggregates and rendering a swarm**; the two layers stay separate:
- **Simulated & replicated (coarse):** cohorts (§14), region/population stats, support, influence —
  plus the **strategically meaningful movers as real entities**: leaders, caravans, raid/war parties
  (few — dozens to low hundreds map-wide).
- **Cosmetic, client-local (fine):** the labouring/living masses are **drawn from replicated cohort
  counts**, not tracked by the sim. Each client animates its own ants from the same numbers, so the
  swarm adds **zero netcode cost** and the coarse server-authoritative replication (§17.3) is
  untouched. This is what the §14 cohort model buys us.
- **Rendering:** sprite **instancing + LOD** — crowd-density blobs/heat at full zoom-out, resolving
  to individual animated figures as you zoom in. Bounded 4-player agent counts keep this tractable.
- **Rejected:** full per-agent simulation (Songs of Syx style) — gorgeous but heavy on perf and a
  serious multiplayer problem (replicating thousands of positions reopens the lockstep trap §17.3
  avoids).

---

## 18. Scope & roadmap

**Status:** workspace scaffolded (sim/client/protocol), Bevy 0.18.1, message-decoupled, builds &
tests green. Client renders a terrain map (sand/oasis/well — water as §13 resource), a settlement
(a clustered town of ~10–15 multi-tile **buildings** with street gaps) whose population cohort is
drawn as a **cosmetic swarm** (§17.4), and one authoritative **leader**
drawn with tick-interpolation — both halves of §17.4 demonstrated. The sim owns a **tile passability
grid** (buildings + water); leader and swarm both collide against it (tile-based, no physics engine).
**Caravans** (real movers, camel-train visuals) run a fixed state machine hauling water from oases
to the settlement's store (§13 water logistics), emitting delivery events. A **territory grid** (§8)
claims each building's footprint + 2 tiles for its owner (tinted gold); the labour swarm is bound to
its owner's territory, while leader/caravans roam freely.

**Threat #1 is scope.** Cross-cultural infection, espionage variety, narcotics ops, full
faction trees, all phases — these are *later*. Build **breadth-last**.

**Vertical slice (build this first):**
- One settlement mode + one scale tier — recommend **settled / village**.
- One loop of each activity (military/economic/religious/leadership).
- The **culture axes + 4 support tracks + influence layer** working end-to-end.
- A handful of **minor powers** with basic behaviour.
- **Single-player vs. dumb AI (or hotseat) first** — prove the indirect-control + culture loop is
  *fun* before any networking. If it isn't fun for one player, multiplayer won't save it.

Then, in rough order: second settlement mode (nomad), more scale tiers, multiplayer (server-auth),
richer minors, cultural ops, succession events, full victory/score systems.

---

## 19. Open questions
- **[OPEN]** Working title.
- **[OPEN]** Culture tuning: axis thresholds, per-action drift amounts, in-world flavour names for the poles (§10).
- **[OPEN]** Starting player archetypes — authored vs. drafted (§7).
- **[OPEN]** Goods list and support-ceiling curve (§13).
- **[OPEN]** Minor tuning: field density/count, final type roster, standing accrual rates, governance-cap formula (§8).
- **[OPEN]** Exact victory thresholds, hold-timers, and score formula (§6.1).
- **[OPEN]** Labour tuning: productivity multipliers per status, revolt-risk formula (slave-ratio ×
  guard-coverage × culture), guard/wage/food upkeep rates (§14).
- **[OPEN]** Netcode library: `bevy_replicon` vs `lightyear` (§17.3). *(Crate split done.)*
- **[OPEN]** Map dimensions (tiles) & region count for 4 players + minors.
- **[OPEN]** Presentation tuning: agent-density caps, LOD zoom thresholds (§17.4).

## 20. Touchstones (study these)
**Systems:** Majesty, Crusader Kings III, Northgard, Patrician/Anno, Total War minor factions, Civ
city-states, Rimworld (legible autonomous agents).
**Presentation/scale:** Songs of Syx (ant-density at scale), The Settlers (autonomous carrier
swarms), Pharaoh/Caesar (dense ancient cities with walkers — setting match), Anno/Banished (dense
settlements).
