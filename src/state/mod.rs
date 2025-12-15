mod location;
mod map;

use map::Map;

pub struct State {
    pub map: Map,
}

impl State {
    pub fn build() -> State {
        let map = Map::new_debug_map(5, 5);
        State { 
            map 
        }    
    }

}
