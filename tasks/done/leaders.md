# Title
Leaders initial implementation

Status: done

## Goal

To set the basis for future leadership system\

## Mechanics

Implement leaders into the game. Leaders will lead all units (for now, later when we add unit types and HQs they may become restricted only to a specific unit type) and will have stats that can influence various different things. For now though I just want to be able to define leaders in the scenario, assign them to specific units (in scenario and in game via command), and have them have stats that are defined in the scenario (which will for now have no effect). I also want a command that will list the leaders for a specific faction (leaders are of course faction based), and a command that will inspect the leaders stats. Implement the same stats that GGWITE2 has for now. Units can be in a state where no leaders are assigned to them.

## Acceptance criteria

Leaders are present in the game; They exist as an object in memory, they can be added by the scenario, can be assigned to units (via scenario config or via command), their stats and assignment can be inspected, we can get a list of all leaders for a faction, leaders show up when doing a detailed inspection of a unit.
