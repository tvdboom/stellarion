<div align="center">

# Stellarion
### A multiplayer space-themed strategy game written in Rust

<br><br>
[![Play](https://gist.githubusercontent.com/cxmeel/0dbc95191f239b631c3874f4ccf114e2/raw/play.svg)](https://tvdboom.itch.io/stellarion)
<br><br>
</div>

<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/map.png?raw=true" alt="Map">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/shop.png?raw=true" alt="Shop">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/mission.png?raw=true" alt="Mission">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/report.png?raw=true" alt="Report">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/combat.png?raw=true" alt="Combat">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/incombat.png?raw=true" alt="InCombat">

<br>

## 📜 Overview

Stellarion is a turn-based strategy game, where players build interstellar empires. Expand your 
colonies, manage resources, and command fleets that engage in strategic battles for dominance of 
the galaxy. The goal of the game is to conquer or destroy the enemy's home planet. If you lose your home 
planet, you lose the game.

### Resources

The game presents three resource types:

- **Metal:** Metal is the most basic resource, used in almost all constructions and ships.
- **Crystal:** Crystal is a more advanced resource, essential for high-level buildings and ships.
- **Deuterium:** Deuterium is the least frequent resource in the galaxy, primarily used for 
  high-level ships and as fuel.

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
if a planet is abandoned, conquered or destroyed.

Moons cannot be colonized (and thus not owned), but they can be controlled. Contrary to planets, 
players can build on a controlled moon. Moons only have a limited number of fields on which to 
build. Increasing the level of the Lunar Base increases the number of fields. Moons don't have 
defenses.

### Units

You can build three types of units on an owned planet:

- **Buildings:** Buildings are used for varied reasons. Core buildings like the mines, Shipyard
  or Factory are essential to expand your empire. Advanced buildings like the Jump gate or 
  Sensor Phalanx should be built more strategically.
- **Ships:** Ships are the backbone of your army. Ship often have unique characteristics that make
  them better or worse suited for certain strategies. Some ships are also stronger or weaker against
  other specific ship types, so try to build your fleet according to your enemy's composition.
- **Defenses:** Defenses are stationary combat units. They have better price-to-stats ratios than
  ships, but are fixed to the planet. Be careful with stacking defenses! War Suns are capable of
  destroying a planet with any defense army. Missiles are also included with the defense units.

### Combat

In combat, there are two sides: the attacker and the defender. There is the possibility that 
the attacker has launched his fleets against a planet with no defense or ships, in which case 
he automatically wins the combat. But otherwise, if the defender has ships or defense on his 
planet, each side will fire upon the enemy. Each combat can have only two outcomes: attacker 
wins or defender wins.

Every unit (ships + defenses) has four basic parameters that affect combat: hull (H), shield (S), 
damage (D), and rapid fire (RF). Combat consists of rounds. In the beginning of each round, every 
unit starts with its shield at its initial value. The hull has the value of previous round 
(initial value of the ship if it's the first round). In each round, all participating units 
randomly choose a target enemy unit. Shots are resolved per ship type in increasing production
order, i.e., the lowest production units shoots first, and the highest production units shoot last
(ships fire before defenses).

For each shooting unit:

1. If it's the first round of a missile strike, the defender's Antiballistic Missiles will fire
   sequentially until they are depleted or no Interplanetary Missiles remain.
2. A random enemy unit is chosen as target. If the unit is a defense unit and the planet has a 
   Planetary Shield with remaining shield, the Planetary Shield is chosen as target instead.
3. If the damage is lower than the enemy's shield, the shield absorbs the shot, and the unit does 
   not lose hull: S = S - W.
4. Else, if W > S, the shield only absorbs part of the shot and the rest of the damage is dealt to 
   the hull: H = H - (W - S) and S = 0.
5. If the shooting unit has rapid fire against the target unit, it has a chance of RF% of choosing 
   another target at random, and repeating the above steps for that new target.
6. All ships with H=0 (no hull points left) are destroyed.
7. If the objective is to destroy the planet and there are no enemy ships left, each attacking 
   War Sun fires a shot with a chance of 10 - 1 * n_turn to hit. If it hits, the planet is
   immediately destroyed and all defenses and buildings with it.
8. If it's the first round of combat and there are any Probes on the attacker's side, they leave 
   combat and fly back to the origin planet (if `combat probes` option disabled).
9. If every unit of a side (attacker or defender) is destroyed, the battle ends with the opposite 
   side winning.


Things to keep in mind:

- Buildings are build before any combat takes place.
- Missions are resolved in arbitrary player order each turn. This means that you cannot know if
  reinforcements will arrive before or after an attack when they both arrive at the destination
  planet the same turn. If reinforcements arrive after the planet has been conquered, the objective
  automatically is transformed in an attack.
- Attacks on the same planet on the same turn are merged per player and per objective. The planet
  of origin becomes the planet that send the largest army. The order of resolution becomes: Missile
  strikes are resolved first, followed by spying missions, and then the remaining, which are
  grouped together following objective priority `Destroy` > `Colonize` > `Attack`.
- An attacking player receives no enemy unit information if all its units are destroyed. If there
  are scout probes, he can only see the number of enemy units prior to combat.
- A defender player receives no enemy unit information if all its units are destroyed and he
  doesn't own the planet.

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
- `Q`: Toggle the audio settings.
- `C`: Show/hide the player's control domain.
- `I`: Show/hide all planet information.
- `H`: Enable/disable information tooltips on hover.
- `B`: Show/hide the shop panel.
- `M`: Show/hide the mission panel.
