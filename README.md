A combat simulation engine/wargame centered around WW2 written in Rust

Side project, worked on in free time

Some really basic functionalities are implemented, the goal is next to work on implementing battle resolution and combat simulation  

Commands:  
new scenarios/basic_scenario.scen - start a new game on an example scenario  
inspect x y - inspect the location at x, y  
inspect offmap_location_name - inspect the offmap location with the given name  
units - prints a list of all units; units detail gives more details  
move x_start y_start x_dest y_dest unit_index - moves the unit with index unit-index from the start to the dest hex  
save path_to_savefile - saves the game state to a file specified  
load path_to_savefile - loads the game state from a file  
exit  
