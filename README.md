<div align="center">

# Stellarion
### A multiplayer space-themed rts game written in Rust

<br><br>
[![Play](https://gist.githubusercontent.com/cxmeel/0dbc95191f239b631c3874f4ccf114e2/raw/play.svg)](https://tvdboom.itch.io/stellarion)
<br><br>
</div>

<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/s1.png?raw=true" alt="Early game">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/s2.png?raw=true" alt="Traits">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/s3.png?raw=true" alt="Late game">
<img src="https://github.com/tvdboom/stellarion/blob/master/assets/images/scenery/s4.png?raw=true" alt="Overview">

<br>

## 📜 Introduction

Stellarion is a real-time strategy game where you control an ant colony. Grow your colony,
expand your nest, gather resources, and fight for survival against scorpions, termites, wasps and
other ant colonies.

<br>

## 🎮 Gameplay

The goal of the game is to conquer all enemy's home planets. If your home planet is conquered, 
you lose the game.

### Planets

#### Owned vs controlled planets

A planet is owned by a player if it's the player's home planet or if it has been colonized by a
Colony Ship. A planet is controlled by a player if the player has military presence on it. If a
planet is owned, it is also controlled by the owner. If a player wins a combat on a planet, but
doesn't colonize it, he gains controls over that planet and the previous owner loses both 
ownership and control.

An owned planet produces resources for its owner. Buildings can only be build and used on owned
planets. For example, a player controlling (but not owning) a planet, cannot use the Phalanx to
see incoming attacks nor use the Jump Gate to move its fleet.

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
randomly choose a target enemy unit.

For each shooting unit:

- A random enemy unit is chosen as target. If the unit is a defense and the planet has a 
  Planetary Shield with remaining shield, the Planetary Shield is chosen as target instead.
- If the damage is lower than the enemy's shield, the shield absorbs the shot, and the unit does 
  not lose hull: S = S - W.
- Else, if W > S, the shield only absorbs part of the shot and the rest of the damage is dealt to 
  the hull: H = H - (W - S) and S = 0.
- If the shooting unit has rapid fire against the target unit, it has a chance of RF% of choosing 
  another target at random, and repeating the above steps for that new target.
- All ships with H=0 (no hull points left) are destroyed.
- If the objective is to destroy the planet and there are no enemy ships left, each attacking 
  War Sun fires a shot with a chance of 20 - n_turn to hit. If it hits, the planet is immediately
  destroyed and all defenses and buildings with it.
- If it's the first round of combat and there are any Probes on the attacker's side, they leave 
  combat and fly back to the origin planet.
- If every unit of a side (attacker or defender) is destroyed, the battle ends with the opposite 
  side winning.


Things to keep in mind:

- Buildings are build before any combat takes place.
- Missions are resolved in arbitrary player order each turn. This means that you cannot know if
  reinforcements will arrive before or after an attack when they both arrive at the destination
  planet the same turn. If reinforcements arrive after the planet has been conquered, the objective
  automatically is transformed in an attack.
- Attacks on the same planet on the same turn are merged per player and per objective. Missile
  strikes are resolved first, followed by spying missions, and then the remaining, which are
  grouped together following objective priority `Destroy` > `Colonize` > `Attack`.
- An attacking player receives no enemy unit information if all its units are destroyed.
- A defender player receives no enemy unit information if all its units are destroyed and he
  doesn't own the planet.

<br>

### Mouse + Key bindings

- `escape`: Enter/exit the in-game menu.
- `w-a-s-d`: Move the map.
- `scroll`: Zoom in/out the map.
- `space`: Center the map on your home planet and select it.
- `tab`: Cycle through your owned planets (if any selected).
- `Q`: Toggle the audio settings.
- `C`: Show/hide the player's control domain.
- `I`: Show/hide all planet information.
- `H`: Enable/disable information tooltips on hover.
- `M`: Show/hide the shop panel.
- `M`: Show/hide the mission panel.
- `mouse forward/backward`: Cycle through the shop/mission menu.
