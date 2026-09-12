<div align="center">

# Stellarion
### A multiplayer space strategy game for browser and desktop

<br><br>
[![Play](https://gist.githubusercontent.com/cxmeel/0dbc95191f239b631c3874f4ccf114e2/raw/play.svg)](https://tvdboom.itch.io/stellarion)
<br><br>
</div>

<img src="assets/images/scenery/map.png" alt="Zoomed-out three-player galaxy with friendly and enemy missions, a Space Dock, and sensor coverage">
<img src="assets/images/scenery/shop.png" alt="Two-player mid-game empire with varied planetary construction">
<img src="assets/images/scenery/mission-planning.png" alt="Planning a balanced attack fleet in a three-player match">
<img src="assets/images/scenery/active-missions.png" alt="Active colonization, attack, and espionage missions in a three-player match">
<img src="assets/images/scenery/combat.png" alt="Live two-player battle with varied fleets, weapon fire, and explosions">
<img src="assets/images/scenery/battle-report.png" alt="Detailed two-player battle report with varied fleets and planetary defenses">

<br>

## 📜 Overview

Stellarion is a turn-based strategy game, where players build interstellar empires. Expand your 
colonies, manage resources, and command fleets that engage in strategic battles for dominance of 
the galaxy. Win by eliminating rival empires or controlling enough of the map. If you lose your
home planet, you are eliminated.

### Win conditions

- **Elimination:** Be the last player who still owns their original home planet. Conquering or
  destroying an opponent's home planet eliminates that player. Mutual elimination is a draw.
- **Territorial control:** Control at least `50% + 50% / n_players` of all surviving planets.
  Destroyed planets lower the threshold.

Victory is checked after all battles and ownership changes for the turn resolve, with no holding
period or countdown. A player who loses their home planet that turn cannot win by territory.
Local Practice has no territorial victory condition. Debug builds can place one to four locally
controlled empires in the same match: select an empire's name in the Players panel to edit its
independent orders. The standard **End turn** control resolves every local player's draft at once.


### Resources

The game presents three resource types:

- **Metal:** Metal is the most basic resource, used in almost all constructions and ships.
- **Crystal:** Crystal is a more advanced resource, essential for high-level buildings and ships.
- **Deuterium:** Deuterium is the least frequent resource in the galaxy, primarily used for 
  high-level ships and as fuel.
- **Energy**: Energy is not a stockpiled resource, but a per-turn capacity. It powers continuously
  operating infrastructure. An energy deficit reduces resource income and Planetary Shield strength.

Planets produce a varying amount of each of these resources. Be aware of your home planet's 
resource production! It should influence the type of strategy you might want to consider for the
game. For example, a home planet with a lack of deuterium forces the player into early expansion 
to be able to fuel its fleet later.

### Planets & Moons

A planet is owned by a player if it's the player's home planet or if it has been colonized by a
Colony Ship. A planet is controlled by a player if the player has military presence on it. If a
planet is owned, it is also controlled by the owner. If a player attacks and wins another planet,
but doesn't colonize it, he gains controls over that planet and the previous owner loses both 
ownership and control.

An owned planet produces resources for its owner. Buildings can only be build and used on owned 
planets. For example, a player controlling (but not owning) a planet, cannot use the Phalanx to 
see incoming attacks nor use the Jump Gate to move its fleet. Combat on an owned planet always 
gives full intelligence on the attacking units to the planet's owner. Also when the fight is lost.
If losing combat on a controlled planet, no intelligence is gained.

There is a limit to the amount of planets that can be owned by a player. Spots are only freed 
if a planet is abandoned, conquered or destroyed. A Senate on the home planet raises this limit
by one planet per level. The base ownership allowance set by the galaxy size and colonizable
planet percentage permits one Senate level per four planets, rounded down, with a minimum of one
and a maximum of five levels.

Moons cannot be colonized (and thus not owned), but they can be controlled. Contrary to planets, 
players can build on a controlled moon. Moons only have a limited number of fields on which to 
build. Increasing the level of the Lunar Base increases the number of fields. Moons don't have 
defenses.

### Mission types

- **Deploy:** Move a fleet to another planet or moon you control.
- **Protect:** A world controller can grant another player planet-specific protection access.
  The invitation is applied immediately and adds Protect alongside the mission choices. A
  protecting fleet joins that world's defense but remains separately owned. Revoking protection
  access applies immediately, and sends both traveling and stationed protection fleets to their
  owner's homeworld. A player cannot launch missions from a stationed protection fleet or target
  the protected world with hostile missions or Orbital Railguns. They may send more protection or
  recall the entire stationed fleet home first.
- **Colonize:** Send ships including at least one Colony Ship to gain ownership of a planet.
  The Colony Ship is consumed, placing a level-one Metal, Crystal, and Deuterium mine on the
  planet.
- **Attack:** Send combat ships against a hostile world. A victory leaves the fleet there and gives
  control, but not ownership. The previous owner loses both ownership and control. Surviving
  buildings remain.
- **Spy:** Gather intelligence on an enemies' strength using the Probe ships. A minimum of five
  probes is required for a spy mission. The more Probes that return, the more intelligence is
  revealed. Spy missions cannot be detected by a Sensor Phalanx and do not reveal their origin.
- **Missile Strike:** Launch Interplanetary Missiles against a planet. They bypass ships and the
  Planetary Shield to hit defenses directly. Surviving missiles are consumed. A strike always
  hits the destination, even if it  becomes friendly. Missile strikes reveal no enemy-unit
  intelligence, cannot be detected by a Sensor Phalanx, and do not reveal their origin.
- **Destroy:** Attack with combat ships including at least one War Sun. After each round with no
  enemy ships or Space Dock remaining, every War Sun has a size-dependent chance to destroy the
  planet (the chance falls in later rounds). The fleet returns whether destruction succeeds. A
  destroyed planet can never be colonized again.


### Units

You can build four types of units on an owned planet:

- **Buildings:** Buildings are used for varied reasons. Core buildings like the mines enhance
  resource production. The Shipyard and Factory allow you to build ships and defenses on a
  planet. Other buildings like the Colonial Administration support your empire.
- **Orbitals:** Orbitals are constructions in space that provide various strategic advantages,
  such as extending sensor range, enabling faster travel, or supporting fleet operations. The
  costly, publicly visible Orbital Railgun can fire once per turn at a planet within its
  level-scaled range; Railguns aimed at one world combine their small, size-dependent destruction
  chances into a synchronized strike. Each Planetary Shield level reduces that chance, with twice
  the reduction while the shield is overloaded.
- **Ships:** Ships are the backbone of your army. Ship often have unique characteristics that make
  them better or worse suited for certain strategies. Some ships are also stronger or weaker against
  other specific ship types, so try to build your fleet according to your enemy's composition.
- **Defenses:** Defenses are stationary combat units. They have better price-to-stats ratios than
  ships, but are fixed to the planet. Be careful with stacking defenses! War Suns are capable of
  destroying a planet with any defense army. Missiles are also included with the defense units.


### Fleet travel

Ships and missiles accelerate throughout each journey. For movement rating `s`, distance covered
after `t` turns is `s * t * (t + 2) / 3` AU. The first turn covers the same distance as before;
each subsequent turn covers an additional `2s/3` AU. Fleets use their slowest unit's rating. Any
owned mission (except a Missile Strike) that is not already returning can be recalled for no
additional cost.


### Combat

Colonial Administration enables a **Fleet withdrawal** setting in the colony's Buildings shop.
It defaults to Off. Level 1 unlocks withdrawal after 75% fleet losses; levels 2 and 3 add 50%
and 25%; level 4 adds immediate withdrawal. Losses are destroyed ship production points relative
to the starting fleet, checked after each round. Once withdrawal begins, the enemy fires one
additional round while withdrawing ships cannot fire back. Stationary defenses keep fighting.
Level 5 removes the final enemy volley, including a clean departure before any shots when set
to Immediately. Only surviving ships leave; a final volley can destroy the entire withdrawing
fleet. Noncombat Colony Ships accompany the evacuation.

Escaped ships form an ordinary **Deploy** mission to the defender's homeworld, using normal
fleet speed, acceleration, and visibility rules. The mission is dated to the preceding turn
and receives one movement step before the post-battle map: a three-turn voyage has two turns
remaining. A one-turn voyage docks that same turn. The homeworld must still belong to the
defender; homeworlds and moons cannot use Colonial Administration. Retreat does not generate
orbital wreckage, and escaped ships are recorded separately from combat casualties.

In combat, there are two sides: the attacker and the defender. There is the possibility that 
the attacker has launched his fleets against a planet with no defense or ships, in which case 
he automatically wins the combat. But otherwise, if the defender has ships or defense on his 
planet, each side will fire upon the enemy. Combat ends in an attacker victory, a defender
victory, or a draw. If both combat armies survive the 100-round limit, the defender keeps
the planet and the surviving attackers return to their origin.

Every unit (ships + defenses) has four basic parameters that affect combat: hull (H), shield (S), 
damage (D), and rapid fire (RF). Combat consists of rounds. In the beginning of each round, every 
unit starts with its shield at its initial value. The hull has the value of previous round
(initial value of the ship if it's the first round). In each round, ordinary ships randomly target
enemy ships and the Space Dock first, falling back to other defenses only when those targets are
gone. Bombers reverse that priority, targeting defenses before ships. Stationary defenses choose
randomly among all enemy units. Shots are resolved per ship type in increasing production order,
i.e., the lowest production units shoot first, and the highest production units shoot last (ships
fire before defenses).

For each shooting unit:

1. If it's the first round of a missile strike, the defender's Antiballistic Missiles will fire
   sequentially until they are depleted or no Interplanetary Missiles remain.
2. A random enemy unit is chosen from the shooter's highest-priority target category. If the unit
   is a defense unit and the planet has a Planetary Shield with remaining shield, the Planetary
   Shield is chosen as target instead.
3. If the damage is lower than the enemy's shield, the shield absorbs the shot, and the unit does 
   not lose hull: S = S - W.
4. Else, if W > S, the shield only absorbs part of the shot and the rest of the damage is dealt to 
   the hull: H = H - (W - S) and S = 0.
5. If the shooting unit has rapid fire against the target unit, it has a chance of RF% of choosing 
   another target at random, and repeating the above steps for that new target.
6. All ships with H=0 (no hull points left) are destroyed.
7. If the objective is to destroy the planet and there are no enemy ships or Space Dock left, each
   attacking War Sun fires a shot with a chance of 10 - 1 * n_turn to hit. If it hits, the planet
   is immediately destroyed and all defenses and buildings with it.
8. If it's the first round of combat and there are any Probes on the attacker's side, they leave 
   combat and fly back to the origin planet (if `combat probes` option disabled).
9. If every unit of a side (attacker or defender) is destroyed, the battle ends with the opposite 
   side winning.


Things to keep in mind:

- Buildings are build before any combat takes place.
- Missions are resolved in arbitrary player order each turn. This means that you cannot know if
  reinforcements will arrive before or after an attack when they both arrive at the destination
  planet the same turn. If reinforcements arrive after the planet has been conquered, the objective
  automatically is transformed in an attack. Protect missions are the exception: every valid
  same-turn Protect arrival is stationed before hostile missions at that world resolve.
- Attacks on the same planet on the same turn are merged per player and per objective. The planet
  of origin becomes the planet that send the largest army. The order of resolution becomes: Missile
  strikes are resolved first, followed by spying missions, and then the remaining, which are
  grouped together following objective priority `Destroy` > `Colonize` > `Attack`.
- An attacking player receives no enemy unit information if all its units are destroyed. If there
  are scout probes, he can only see the number of enemy units prior to combat.
- A defender player receives no enemy unit information if all its units are destroyed and he
  doesn't own the planet.
- Protection fleets arriving on the same turn as an attack defend first.

<br>

### Mouse + Key bindings

- `escape`: Enter/exit the in-game menu.
- `enter`: Send mission (when in tab).
- `ctrl + enter`: Finish turn.
- `w-a-s-d`: Move the map.
- `scroll`: Zoom in/out the map.
- `space`: Center the map on your home planet and select it.
- `tab / mouse forward-backward`: Cycle through the shop/mission menu or rounds in a combat report.
- `ctrl + tab`: Cycle through your owned planets (if any selected).
- During combat: `space` pauses, `ctrl + left/right` changes speed, and `ctrl + shift + left/right` jumps rounds.
- `Q`: Toggle the audio settings.
- `C`: Show/hide the player's control domain.
- `I`: Show/hide all planet information.
- `H`: Enable/disable information tooltips on hover.
- `B`: Show/hide the shop panel.
- `M`: Show/hide the mission panel.
