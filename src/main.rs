use std::cmp;

use rand::Rng;
use tcod::colors::*;
use tcod::console::*;
use tcod::input::{self, Event, Key, Mouse};
use tcod::map::{FovAlgorithm, Map as FovMap};

// Actual size of the window
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

// Size of the map
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

// Sizes and coordinates relevant for the GUI
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;
const INVENTORY_WIDTH: i32 = 50;

// Parameters for dungeon generator
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;
const MAX_ROOM_MONSTERS: i32 = 3;
const MAX_ROOM_ITEMS: i32 = 2;

const HEAL_AMOUNT: i32 = 4;
const LIGHTNING_DAMAGE: i32 = 40;
const LIGHTNING_RANGE: i32 = 5;
const CONFUSE_RANGE: i32 = 8;
const CONFUSE_NUM_TURNS: i32 = 10;
const FIREBALL_RADIUS: i32 = 3;
const FIREBALL_DAMAGE: i32 = 12;

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic; // Default fov algorithm
const FOV_LIGHT_WALLS: bool = true; // Light walls or not
const TORCH_RADIUS: i32 = 10;

const LIMIT_FPS: i32 = 20; // Number of times per second game loop will be executed

const COLOR_DARK_WALL: Color = Color {r: 0, g: 0, b: 100};
const COLOR_LIGHT_WALL: Color = Color {r: 130, g: 110, b: 50};
const COLOR_DARK_GROUND: Color = Color {r: 50, g: 50, b: 150};
const COLOR_LIGHT_GROUND: Color = Color {r: 200, g: 180, b: 50};

// Player will always be the first object
const PLAYER: usize = 0;

struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    key: Key,
    mouse: Mouse
}

type Map = Vec<Vec<Tile>>;

struct Messages {
    messages: Vec<(String, Color)>
}

impl Messages {
    pub fn new() -> Self {
        Self {
            messages: vec![]
        }
    }

    /// Add the new message as a tuple, with the text and the color
    pub fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        self.messages.push((message.into(), color));
    }

    /// Create a 'DoubleEndedIterator' over the messages
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(String, Color)> {
        self.messages.iter()
    }
}

struct Game {
    map: Map,
    messages: Messages,
    inventory: Vec<Object>
}

/// A tile of the map and its properties
#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool
}

impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            explored: false,
            block_sight: false
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            explored: false,
            block_sight: true
        }
    }
}

/// A rectangle on the map, used to characterise a room.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect {
            x1: x,
            y1: y,
            x2: x + w,
            y2: y + h
        }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        (center_x, center_y)
    }

    pub fn intersects_with(&self, other: &Rect) -> bool {
        // Returns true if this rectangle intersects with another one
        (self.x1 <= other.x2)
            && (self.x2 >= other.x1)
            && (self.y1 <= other.y2)
            && (self.y2 >= other.y1)
    }
}

/// This is a generic object: player, monster, item, stairs, etc.
/// It is always represented by a character on screen.
#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
    name: String,
    blocks: bool,
    alive: bool,
    fighter: Option<Fighter>,
    ai: Option<Ai>,
    item: Option<Item>
}

impl Object {
    pub fn new(x:i32, y: i32, char: char, name: &str, color: Color, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            char: char,
            color: color,
            name: name.into(),
            blocks: blocks,
            alive: false,
            fighter: None,
            ai: None,
            item: None
        }
    }

    /// Set the color and then draw the character that represents this object at its position
    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// Return the distance to another object
    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    pub fn take_damage(&mut self, damage: i32, game: &mut Game) {
        // Apply damage if possible
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }

        // Check for death, call the death function
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self, game);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object, game: &mut Game) {
        // A simple formula for attack damage
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);
        if damage > 0 {
            // Make the target take some damage
            game.messages.add(format!("{} attacks {} for {} hit points.", self.name, target.name, damage), WHITE);
            target.take_damage(damage, game);
        } else {
            game.messages.add(format!("{} attacks {} but it has no effect!", self.name, target.name), WHITE);
        }
    }

    /// Heal by the given amount, without going over the maximum
    pub fn heal(&mut self, amount: i32) {
        if let Some(ref mut fighter) = self.fighter {
            fighter.hp += amount;
            if fighter.hp > fighter.max_hp {
                fighter.hp = fighter.max_hp;
            }
        }
    }

    /// Return the distance to some coordinates
    pub fn distance(&self, x: i32, y: i32) -> f32 {
        (((x - self.x).pow(2) + (y - self.y).pow(2)) as f32).sqrt()
    }
}

/// Move by the given amount, if the destination is not blocked
fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x + dx, y + dy, map, objects) {
        objects[id].set_pos(x + dx, y + dy);
    }
}

/// Move an object towards a position
fn move_towards(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    // Vector from this object to the target, and distance
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    // Normalize it to length 1 (preserving direction), then round it and convert
    // to integer so the movement is restricted to the map grid
    let dx = (dx as f32/ distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, map, objects);
}

/// Mutably borrow two *separate* elements from the given slice.
/// Panics when the indexes are equal or out of bounds.
fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_index != second_index);
    let split_at_index = cmp::max(first_index, second_index);
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}

/// Add item to player's inventory and remove it from the map
fn pick_item_up(object_id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    if game.inventory.len() >= 26 {
        game.messages.add(
            format!(
                "Your inventory is full, cannot pick up {}.",
                objects[object_id].name
            ),
            RED
        );
    } else {
        let item = objects.swap_remove(object_id);
        game.messages.add(format!("You picked up a {}!", item.name), GREEN);
        game.inventory.push(item);
    }
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    // First test the map tile
    if map[x as usize][y as usize].blocked {
        return true;
    }

    // Now check for any blocking objects
    objects
        .iter()
        .any(|object| object.blocks && object.pos() == (x, y))
}

// Combat-related properties and methods (monster, player, npc)
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster
}

impl DeathCallback {
    fn callback(self, object: &mut Object, game: &mut Game) {
        use DeathCallback::*;
        let callback = match self {
            Player => player_death,
            Monster => monster_death
        };
        callback(object, game);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Ai {
    Basic,
    Confused {
        previous_ai: Box<Ai>,
        num_turns: i32
    }
}

fn ai_take_turn(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) {
    use Ai::*;
    if let Some(ai) = objects[monster_id].ai.take() {
        let new_ai = match ai {
            Basic => ai_basic(monster_id, tcod, game, objects),
            Confused {
                previous_ai,
                num_turns
            } => ai_confused(monster_id, tcod, game, objects, previous_ai, num_turns)
        };

        objects[monster_id].ai = Some(new_ai);
    }
}

fn ai_basic(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut[Object]) -> Ai {
    // A basic monster takes its turn. If you can see it, it can see you
    let (monster_x, monster_y) = objects[monster_id].pos();
    if tcod.fov.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            // Move towards player if far away
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, &game.map, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            // Close enough, attack (if the player is still alive)
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player, game);
        }
    }
    Ai::Basic
}

fn ai_confused(monster_id: usize, _tcod: &Tcod, game: &mut Game, objects: &mut [Object], previous_ai: Box<Ai>, num_turns: i32) -> Ai {
    if num_turns >= 0 {
        // Still confused, so move in a random direction, and decrease the number of turns confused
        move_by(
            monster_id,
            rand::thread_rng().gen_range(-1, 2),
            rand::thread_rng().gen_range(-1, 2),
            &game.map,
            objects
        );

        Ai::Confused {
            previous_ai: previous_ai,
            num_turns: num_turns - 1
        }
    } else {
        // Restore the previous AI (this one will be deleted)
        game.messages.add(
            format!("The {} is no longer confused!", objects[monster_id].name),
            RED
        );
        *previous_ai
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Heal,
    Lightning,
    Confuse,
    Fireball
}

enum UseResult {
    UsedUp,
    Cancelled
}

fn use_item(inventory_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut[Object]) {
    use Item::*;

    // Call the "use" function, if it is defined
    if let Some(item) = game.inventory[inventory_id].item {
        let on_use = match item {
            Heal => cast_heal,
            Lightning => cast_lightning,
            Confuse => cast_confuse,
            Fireball => cast_fireball
        };

        match on_use(inventory_id, tcod, game, objects) {
            UseResult::UsedUp => {
                // Destroy after use, unless it was cancelled for some reason
                game.inventory.remove(inventory_id);
            }

            UseResult::Cancelled => {
                game.messages.add("Cancelled", WHITE);
            }
        }
    } else {
        game.messages.add(
            format!("The {} cannot be used.", game.inventory[inventory_id].name),
            WHITE
        );
    }
}

fn drop_item(inventory_id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    let mut item = game.inventory.remove(inventory_id);
    item.set_pos(objects[PLAYER].x, objects[PLAYER].y);
    game.messages.add(format!("You dropped a {}.", item.name), YELLOW);
    objects.push(item);
}

/// Return the position of a tile left-clicked in player's FOV (optionally in a range), or (None, None) if right-clicked
fn target_tile(tcod: &mut Tcod, game: &mut Game, objects: &[Object], max_range: Option<f32>) -> Option<(i32, i32)> {
    use tcod::input::KeyCode::Escape;
    loop {
        // Render the screen. This erases the inventory and shows the names of objects under the mouse
        tcod.root.flush();
        let event = input::check_for_event(input::KEY_PRESS | input::MOUSE).map(|e| e.1);
        match event {
            Some(Event::Mouse(m)) => tcod.mouse = m,
            Some(Event::Key(k)) => tcod.key = k,
            None => tcod.key = Default::default()
        }
        render_all(tcod, game, objects, false);

        let (x, y) = (tcod.mouse.cx as i32, tcod.mouse.cy as i32);

        // Accept the target if the player clicked in FOV, and in case a range is specified, if it's in that range
        let in_fov = (x < MAP_WIDTH) && (y < MAP_HEIGHT) && tcod.fov.is_in_fov(x, y);
        let in_range = max_range.map_or(true, |range| objects[PLAYER].distance(x, y) <= range);
        if tcod.mouse.lbutton_pressed && in_fov && in_range {
            return Some((x, y));
        }

        if tcod.mouse.rbutton_pressed || tcod.key.code == Escape {
            return None; // Cancel if the player right-clicked or pressed Escape
        }
    }
}

/// Returns a clicked monster inside FOV up to a range, or None if right-clicked
fn target_monster(tcod: &mut Tcod, game: &mut Game, objects: &[Object], max_range: Option<f32>) -> Option<usize> {
    loop {
        match target_tile(tcod, game, objects, max_range) {
            Some((x, y)) => {
                // Return the first clicked monster, otherwise continue looping
                for (id, obj) in objects.iter().enumerate() {
                    if obj.pos() == (x, y) && obj.fighter.is_some() && id != PLAYER {
                        return Some(id);
                    }
                }
            }

            None => return None
        }
    }
}

/// Find closest enemy, up to a maximum range, and in the player's FOV
fn closest_monster(tcod: &Tcod, objects: &[Object], max_range: i32) -> Option<usize> {
    let mut closest_enemy = None;
    let mut closest_dist = (max_range + 1) as f32; // Start with (slightly more than) max range

    for(id, object) in objects.iter().enumerate() {
        if id != PLAYER && object.fighter.is_some() && object.ai.is_some() && tcod.fov.is_in_fov(object.x, object.y) {
            // Calculate distance between this object and the player
            let dist = objects[PLAYER].distance_to(object);
            if dist < closest_dist {
                // It's closer, so remember it
                closest_enemy = Some(id);
                closest_dist = dist;
            }
        }
    }

    closest_enemy
}

fn cast_heal(_inventory_id: usize, _tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) -> UseResult {
    // Heal the player
    if let Some(fighter) = objects[PLAYER].fighter {
        if fighter.hp == fighter.max_hp {
            game.messages.add("You are already at full health.", RED);
            return UseResult::Cancelled;
        }

        game.messages.add("Your wounds start to feel better!", LIGHT_VIOLET);
        objects[PLAYER].heal(HEAL_AMOUNT);
        return UseResult::UsedUp;
    }

    UseResult::Cancelled
}

fn cast_lightning(_inventory_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) -> UseResult {
    // Find closest enemy (inside a maxium range) and damage it
    let monster_id = closest_monster(tcod, objects, LIGHTNING_RANGE);
    if let Some(monster_id) = monster_id {
        // Zap it!
        game.messages.add(
            format!(
                "A lightning bolt strikes the {} with a loud thunder! \
                The damage is {} hit points.",
                objects[monster_id].name, LIGHTNING_DAMAGE
            ),
            LIGHT_BLUE
        );
        objects[monster_id].take_damage(LIGHTNING_DAMAGE, game);
        UseResult::UsedUp
    } else {
        // No enemy found within maximum range
        game.messages.add("No enemy is close enough to strike.", RED);
        UseResult::Cancelled
    }
}

fn cast_confuse(_inventory_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut[Object],) -> UseResult {
    // Find closest enemy in range and confuse it
    game.messages.add("Left-click an anemy to confuse it, or right-click to cancel.", LIGHT_CYAN);
    let monster_id = target_monster(tcod, game, objects, Some(CONFUSE_RANGE as f32));
    if let Some(monster_id) = monster_id {
        let old_ai = objects[monster_id].ai.take().unwrap_or(Ai::Basic);

        // Replace monster's AI with confused one; after some turns it will restore old AI
        objects[monster_id].ai = Some(Ai::Confused {
            previous_ai: Box::new(old_ai),
            num_turns: CONFUSE_NUM_TURNS
        });

        game.messages.add(
            format!(
                "The eyes of {} look vacant, as he starts to stumble around!",
                objects[monster_id].name
            ),
            LIGHT_GREEN
        );

        UseResult::UsedUp
    } else {
        // No enemy found within max range
        game.messages.add("No enemy is close enough to strike.", RED);
        UseResult::Cancelled
    }
}

fn cast_fireball(_inventory_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) -> UseResult {
    // Ask the player for a target tile to throw a fireball at
    game.messages.add("Left-click a target tile for the fireball, or right-click to cancel.", LIGHT_CYAN);
    let (x, y) = match target_tile(tcod, game, objects, None) {
        Some(tile_pos) => tile_pos,
        None => return UseResult::Cancelled
    };

    game.messages.add(
        format!(
            "The fireball explodes, burning everything within {} tiles!",
            FIREBALL_RADIUS
        ),
        ORANGE
    );

    for obj in objects {
        if obj.distance(x, y) <= FIREBALL_RADIUS as f32 && obj.fighter.is_some() {
            game.messages.add(
                format!(
                    "The {} gets burned for {} hit points.",
                    obj.name, FIREBALL_DAMAGE
                ),
                ORANGE
            );
            obj.take_damage(FIREBALL_DAMAGE, game);
        }
    }

    UseResult::UsedUp
}

fn create_room(room: Rect, map: &mut Map) {
    // Go through the tiles in the rectangle and make them passable
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    // Horizontal tunnel. 'min()' and 'max()' are used in case 'x1 > x2'
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    // Vertical tunnel
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn make_map(objects: &mut Vec<Object>) -> Map {
    // Fill map with "blocked" tiles
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        // Random width and height
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);

        // Random position without going out of the boundaries of the map
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let new_room = Rect::new(x, y, w, h);

        // Run through the other rooms and see if they intersect with this one
        let failed = rooms
            .iter()
            .any(|other_room| new_room.intersects_with(other_room));

        if !failed {
            // This means there are no intersections, so this room is valid
            // Paint it to the map's tiles
            create_room(new_room, &mut map);

            // Add some content to this room, such as monsters
            place_objects(new_room, &map, objects);

            // Center coordinates of the new room, will be useful later
            let (new_x, new_y) = new_room.center();

            if rooms.is_empty() {
                // This is the first room, where the player starts at
                objects[PLAYER].set_pos(new_x, new_y);
            } else {
                // All rooms after the first: connect it to the previous room with a tunnel

                // Center coordinates of the previous room
                let (prev_x, prev_y) = rooms[rooms.len() - 1].center();

                // Toss a coin (random bool value)
                if rand::random() {
                    // First move horizontally, then vertically
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    create_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    // First move vertically, then horizontally
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    create_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }

            // Finally, append the new room to the list
            rooms.push(new_room);
        }
    }

    map
}

fn place_objects(room: Rect, map: &Map, objects: &mut Vec<Object>) {
    // Choose random number of monsters
    let num_monsters = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

    for _ in 0..num_monsters {
        // Choose random spot for this monster
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        // only place it if the tile is not blocked
        if !is_blocked(x, y, map, objects) {
            // 80% chance of getting an orc
            // Generate the monster
            let mut monster = if rand::random::<f32>() < 0.8 {
                // Create an orc
                let mut orc = Object::new(x, y, 'o', "orc", DESATURATED_GREEN, true);
                orc.fighter = Some(Fighter {
                    max_hp: 10,
                    hp: 10,
                    defense: 0,
                    power: 3,
                    on_death: DeathCallback::Monster
                });
                orc.ai = Some(Ai::Basic);
                orc
            } else {
                // Create a troll
                let mut troll = Object::new(x, y, 'T', "troll", DARKER_GREEN, true);
                troll.fighter = Some(Fighter {
                    max_hp: 16,
                    hp: 16,
                    defense: 1,
                    power: 4,
                    on_death: DeathCallback::Monster
                });
                troll.ai = Some(Ai::Basic);
                troll
            };
    
            monster.alive = true;
            objects.push(monster);
        }
    }

    // Choose random number of items
    let num_items = rand::thread_rng().gen_range(0, MAX_ROOM_ITEMS + 1);

    for _ in 0..num_items{
        // Choose random spot for this item
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        // Only place it if the tile is not blocked
        if !is_blocked(x, y, map, objects) {
            let dice = rand::random::<f32>();
            let item = if dice < 0.7 {
                // Create a healing potion (70% chance)
                let mut object = Object::new(x, y, '!', "healing potion", VIOLET, false);
                object.item = Some(Item::Heal);
                object
            } else if dice < 0.7 + 0.1 {
                // Create a lightning bolt scroll (10% chance)
                let mut object = Object::new(x, y, '#', "scroll of lightning bolt", LIGHT_YELLOW, false);
                object.item = Some(Item::Lightning);
                object
            } else if dice < 0.7 + 0.1 + 0.1 {
                // Create a fireball scroll (10% chance)
                let mut object = Object::new(x, y, '#', "scroll of fireball", LIGHT_YELLOW, false);
                object.item = Some(Item::Fireball);
                object
            } else {
                // Create a confuse scroll (10% chance)
                let mut object = Object::new(x, y, '#', "scroll of confusion", LIGHT_YELLOW, false);
                object.item = Some(Item::Confuse);
                object
            };
            objects.push(item);
        }
    }
}

/// Render a bar (HP, experience, etc.).
fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: Color,
    back_color: Color
) {
    // First calculate the width of the bar
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    // Render the background first
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    // Now render the bar on top
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    // Finally, some centered text with the values
    panel.set_default_foreground(WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        &format!("{}: {}/{}", name, value, maximum)
    );
}

/// Return a string with the names of all objects under the mouse
fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    // Create a list with the names of all objects at the mouse's coordinates and in FOV
    let names = objects
        .iter()
        .filter(|obj| obj.pos() == (x, y) && fov_map.is_in_fov(obj.x, obj.y))
        .map(|obj| obj.name.clone())
        .collect::<Vec<_>>();

    names.join(", ") // Join the names, separated by commas
}

fn render_all(tcod: &mut Tcod, game: &mut Game, objects: &[Object], fov_recompute: bool) {
    if fov_recompute {
        // Recompute fov if needed (the player moved or something)
        let player = &objects[PLAYER];
        tcod.fov.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    }

    // Go through all tiles, and set their background color
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = tcod.fov.is_in_fov(x, y);
            let wall = game.map[x as usize][y as usize].block_sight;
            let color = match(visible, wall) {
                // Outside fov
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                // Inside fov
                (true, true) => COLOR_LIGHT_WALL,
                (true, false) => COLOR_LIGHT_GROUND
            };
            let explored = &mut game.map[x as usize][y as usize].explored;
            if visible {
                // Since it's visible, explore it
                *explored = true;
            }
            if *explored {
                // Show explored tiles only (any visible tile is explored already)
                tcod.con.set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }

    // Sort so than non-blocking objects come first
    let mut to_draw: Vec<_> = objects.iter().filter(|o| tcod.fov.is_in_fov(o.x, o.y)).collect();
    to_draw.sort_by(|o1, o2| {o1.blocks.cmp(&o2.blocks)});

    // Draw all objects in the list
    for object in &to_draw {
        object.draw(&mut tcod.con);
    }

    // Blit the contents of "con" to the root console and present it
    blit(
        &tcod.con,
        (0, 0),
        (MAP_WIDTH, MAP_HEIGHT),
        &mut tcod.root,
        (0, 0),
        1.0,
        1.0
    );

    // Prepare to render the GUI panel
    tcod.panel.set_default_background(BLACK);
    tcod.panel.clear();

    // Print the game messages, one line at a time
    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    // Show the player's stats
    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp);
    render_bar(
        &mut tcod.panel,
        1,
        1,
        BAR_WIDTH,
        "HP",
        hp,
        max_hp,
        LIGHT_RED,
        DARK_RED
    );

    // Display names of objects under the mouse
    tcod.panel.set_default_foreground(LIGHT_GREY);
    tcod.panel.print_ex(
        1,
        0,
        BackgroundFlag::None,
        TextAlignment::Left,
        get_names_under_mouse(tcod.mouse, objects, &tcod.fov)
    );

    // Blit the contents of 'panel' to the root console
    blit(
        &tcod.panel,
        (0, 0),
        (SCREEN_WIDTH, SCREEN_HEIGHT),
        &mut tcod.root,
        (0, PANEL_Y),
        1.0,
        1.0
    );
}

fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    // The coordinates the player is moving to/attacking
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    // Try to find an attackable object there
    let target_id = objects.iter().position(|object| object.fighter.is_some() && object.pos() == (x, y));

    // Attack if target found, move otherwise
    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target, game);
        }
        None => {
            move_by(PLAYER, dx, dy, &game.map, objects)
        }
    }
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, root: &mut Root) -> Option<usize> {
    assert!(
        options.len() <= 26,
        "Cannot have a menu with more than 26 options."
    );

    // Calculate total height for the header (after auto-wrap) and one line per option
    let header_height = root.get_height_rect(0, 0, width, SCREEN_HEIGHT, header);
    let height = options.len() as i32 + header_height;

    // Create an offscreen console that represents the menu's window
    let mut window = Offscreen::new(width, height);

    // Print the header, with auto-wrap
    window.set_default_foreground(WHITE);
    window.print_rect_ex(0, 0, width, height, BackgroundFlag::None, TextAlignment::Left, header);

    // Print all the options
    for(index, option_text) in options.iter().enumerate() {
        let menu_letter = (b'a' + index as u8) as char;
        let text = format!("({}) {}", menu_letter, option_text.as_ref());
        window.print_ex(0, header_height + index as i32, BackgroundFlag::None, TextAlignment::Left, text);
    }

    // Blit the contents of the "window" to the root console
    let x = SCREEN_WIDTH / 2 - width / 2;
    let y = SCREEN_HEIGHT / 2 - height / 2;
    blit(&window, (0, 0), (width, height), root, (x, y), 1.0, 0.7);

    // Present the root console to the player and wait for a keypress
    root.flush();
    let key = root.wait_for_keypress(true);

    // Convert the ASCII code to an index; if it corresponds to an option, return it
    if key.printable.is_alphabetic() {
        let index = key.printable.to_ascii_lowercase() as usize - 'a' as usize;
        if index < options.len() {
            Some(index)
        } else {
            None
        }
    } else {
        None
    }
}

fn inventory_menu(inventory: &[Object], header: &str, root: &mut Root) -> Option<usize> {
    // Show a menu with each item of the inventory as an option
    let options = if inventory.len() == 0 {
        vec!["Inventory is empty.".into()]
    } else {
        inventory.iter().map(|item| item.name.clone()).collect()
    };

    let inventory_index = menu(header, &options, INVENTORY_WIDTH, root);

    // If an item was chosen, return it
    if inventory.len() > 0 {
        inventory_index
    } else {
        None
    }
}

fn handle_keys(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) -> PlayerAction {
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    let player_alive = objects[PLAYER].alive;
    match (tcod.key, tcod.key.text(), player_alive) {
        // Toggle fullscreen
        (
            Key {
                code: Enter,
                alt: true,
                ..
            },
            _,
            _
        ) => {
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            DidntTakeTurn
        }

        // Exit game
        (Key {code: Escape, ..}, _, _) => Exit,

        // Movement
        (Key {code: Up, ..}, _, true) => {
            player_move_or_attack(0, -1, game, objects);
            TookTurn
        }
        (Key {code: Down, ..}, _, true) => {
            player_move_or_attack(0, 1, game, objects);
            TookTurn
        }
        (Key {code: Left, ..}, _, true) => {
            player_move_or_attack(-1, 0, game, objects);
            TookTurn
        }
        (Key {code: Right, ..}, _, true) => {
            player_move_or_attack(1, 0, game, objects);
            TookTurn
        }

        // Pick up an item
        (Key {code: Text, ..}, "g", true) => {
            let item_id = objects
                .iter()
                .position(|object| object.pos() == objects[PLAYER].pos() && object.item.is_some());

            if let Some(item_id) = item_id {
                pick_item_up(item_id, game, objects);
            }
            DidntTakeTurn
        }

        // Show the inventory: if an item is selected, use it
        (Key {code: Text, ..}, "i", true) => {
            let inventory_index = inventory_menu(
                &game.inventory,
                "Press the key next to an item to use it, or any other to cancel.\n",
                &mut tcod.root
            );
            
            if let Some(inventory_index) = inventory_index {
                use_item(inventory_index, tcod, game, objects);
            }

            DidntTakeTurn
        }

        // Show the inventory: if an item is selected, drop it
        (Key {code: Text, ..}, "d", true) => {
            let inventory_index = inventory_menu(
                &game.inventory,
                "Press the key next to an item to drop it, or any other to cancel. \n",
                &mut tcod.root
            );

            if let Some(inventory_index) = inventory_index {
                drop_item(inventory_index, game, objects);
            }

            DidntTakeTurn
        }

        _ => DidntTakeTurn
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit
}

fn player_death(player: &mut Object, game: &mut Game) {
    // The game ended
    game.messages.add("You died!", RED);

    // For added effect, transform the player into a corpse
    player.char = '%';
    player.color = DARK_RED;
}

fn monster_death(monster: &mut Object, game: &mut Game) {
    // Transform it into a nasty corpse!
    // It doesn't block, can't be attacked, and doesn't move
    game.messages.add(format!("{} is dead!", monster.name), ORANGE);
    monster.char = '%';
    monster.color = DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

fn new_game(tcod: &mut Tcod) -> (Game, Vec<Object>) {
    // Create object representing the player
    let mut player = Object::new(0, 0, '@', "player", WHITE, true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30,
        defense: 2,
        power: 5,
        on_death: DeathCallback::Player
    });

    // List of objects with just the player
    let mut objects = vec![player];

    let mut game = Game {
        // Generate map (at this point it's not drawn to the screen)
        map: make_map(&mut objects),
        messages: Messages::new(),
        inventory: vec![]
    };

    initialise_fov(tcod, &game.map);

    // A warm welcoming message!
    game.messages.add("Welcome, stranger! Prepare to perish in the Tombs of the Ancient Kings.", RED);

    (game, objects)
}

fn initialise_fov(tcod: &mut Tcod, map: &Map) {
    // Populate the fov map, according to the generated map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            tcod.fov.set(
                x,
                y,
                !map[x as usize][y as usize].block_sight,
                !map[x as usize][y as usize].blocked
            )
        }
    }
}

fn play_game(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) {
    // Force FOV recompute first time through the game loop
    let mut previous_player_position = (-1, -1);

    // Main game loop
    while !tcod.root.window_closed() {
        // Clear the screen of the previous frame
        tcod.con.clear();

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => tcod.key = k,
            _ => tcod.key = Default::default()
        }

        // Render the screen
        let fov_recompute = previous_player_position != (objects[PLAYER].x, objects[PLAYER].y);
        render_all(tcod, game, &objects, fov_recompute);

        tcod.root.flush();

        // Handle keys and exit game if needed
        previous_player_position = objects[PLAYER].pos();
        let player_action = handle_keys(tcod, game, objects);
        if player_action == PlayerAction::Exit {
            break;
        }

        // Let monsters take their turn
        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &tcod, game, objects);
                }
            }
        }
    }
}

fn main() {
    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rustlike")
        .init();

    let mut tcod = Tcod {
        root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        key: Default::default(),
        mouse: Default::default()
    };

    let (mut game, mut objects) = new_game(&mut tcod);
    play_game(&mut tcod, &mut game, &mut objects);
}
