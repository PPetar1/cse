# Title
Implement multiple players per faction; Split faction from player

Status: done

## Goal

Faction and player should be separate entities, and we should make this clear by allowing multiple players per faction

## Mechanics

Faction and player should be split as separate entities. We should allow more than one player per faction to exist, for now there will be no limitation on what each player can controll. The players will play one after the other for now (in the future we should allow the players to have separate roles and control different aspects (industry, army, air...) and also be able to play simultaniously for one faction, put this into ideas.md please). The order that the players play in can be just naturally got from the order they are listed in scenario file and loaded. Player should for now have just a faction he is controlling and his name. When you go into a game, the faction on turn should itterate its list of players allowing them to play in turns. When a player goes end_turn, the next player should take over or if there are no players for that faction left the next faction's turn should start. All end of turn things (like for example doctrine adjustments) that were executed at the end of factions turn should be executed now after all the players for that faction play their turn. Units should not refresh MP and similar things between players turns, only at the start of faction turn. So what we have now will be called faction turn (at least in the IGOUGO variant) and that should be split into multiple player turns. One player ending turn should just hand over the game state to another player, nothing else. Everywhere in code where we reference faction we should now have true faction referenced, players are just the controllers, they should not influense the faction nor have stats or things like that. If a scenario doesn't define a player, singular default player is used.

## Acceptance criteria

Factions and players are separate entities and multiple players can play. One scenario has 2 players added for one faction so that we can test this in play.
