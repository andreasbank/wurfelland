use glfw::{Action, Context, Key};
use std::os::raw::c_void;
use std::ptr;
use std::time::Instant;

mod save;

mod game;
use game::Player;

mod camera;
use camera::Camera;

mod world;
use world::{World, ItemEntity, ItemType, Chicken, Pig, nearest_entity_hit, EntityRegistry};

mod renderer;
use renderer::ChunkRenderer;
use renderer::ShadowPass;
use renderer::SunRenderer;
use renderer::SkyRenderer;
use renderer::MinimapRenderer;
use renderer::ClockRenderer;
use renderer::shadow_pass::{CASCADE_ENDS, NUM_CASCADES};
use renderer::crosshair_renderer;
use renderer::HealthBar;
use renderer::MenuRenderer;
use renderer::MainMenuRenderer;
use renderer::BlockOutlineRenderer;
use renderer::PlayerRenderer;
use renderer::player_renderer::PlayerDrawMode;
use renderer::CrackRenderer;
use renderer::ItemRenderer;
use renderer::HotbarRenderer;
use renderer::BagRenderer;
use renderer::BuildMenuRenderer;
use renderer::ConsoleRenderer;
use renderer::console_renderer::ConsoleAction;
use renderer::EntityRenderer;
use renderer::MultiplayerMenuRenderer;
use renderer::OptionsMenuRenderer;
use renderer::UnderwaterRenderer;
use renderer::LoadMenuRenderer;

mod net;
use net::{GameServer, GameClient};
use net::messages::{ServerMessage, SERVER_PORT};

#[derive(PartialEq, Eq)]
enum GameState {
    MainMenu,
    LoadMenu,
    MultiplayerMenu,
    LoadingGame,
    LoadingMenu, // transitioning from Playing back to the main menu
    Playing,
}

const CHUNK_RADII: [i32; 5] = [4, 6, 8, 10, 12];

#[derive(Clone, Copy, PartialEq)]
enum FogDistance { Near, Normal, Far, Off }

impl FogDistance {
    fn fog_params(self) -> (f32, f32) {
        match self {
            Self::Near   => (48.0,   64.0),
            Self::Normal => (80.0,   96.0),
            Self::Far    => (112.0, 128.0),
            Self::Off    => (9990.0, 9999.0),
        }
    }
    fn as_idx(self) -> usize {
        match self { Self::Near => 0, Self::Normal => 1, Self::Far => 2, Self::Off => 3 }
    }
    fn next(self) -> Self {
        match self {
            Self::Near => Self::Normal, Self::Normal => Self::Far,
            Self::Far  => Self::Off,   Self::Off    => Self::Near,
        }
    }
}


fn main() {
    let mut glfw = glfw::init_no_callbacks().unwrap();

    let (mut window, events) = glfw
        .create_window(1600, 1200, "Wurfelland", glfw::WindowMode::Windowed)
        .unwrap();

    window.make_current();
    window.set_key_polling(true);
    window.set_cursor_pos_polling(true);
    window.set_mouse_button_polling(true);
    window.set_char_polling(true);

    gl::load_with(|symbol| {
        if let Some(addr) = window.get_proc_address(symbol) {
            addr as *const c_void
        } else {
            ptr::null()
        }
    });

    unsafe {
        let chunk_renderer  = ChunkRenderer::new().unwrap();
        let mut shadow_pass = ShadowPass::new().unwrap();
        let sun_renderer    = SunRenderer::new().unwrap();
        let sky_renderer    = SkyRenderer::new("sky.hdr").unwrap();
        let mut minimap     = MinimapRenderer::new();
        let clock_renderer  = ClockRenderer::new();
        println!("OpenGL initialized");

        gl::Enable(gl::DEPTH_TEST);

        // ── World + player (no blocking — menu loads it incrementally) ─────────
        let mut world  = World::new(12, 0xDEAD_C0DE);
        let mut player = Player::new();

        // Kick off background chunk generation around spawn
        world.update([8.0, 28.0, 8.0]);

        // ── All other renderers ────────────────────────────────────────────────
        let crosshair_renderer  = crosshair_renderer::Crosshair::new();
        let health_bar          = HealthBar::new();
        let mut menu_renderer   = MenuRenderer::new();
        let main_menu           = MainMenuRenderer::new();
        let outline_renderer    = BlockOutlineRenderer::new();
        let player_renderer     = PlayerRenderer::new();
        let crack_renderer      = CrackRenderer::new();
        let item_renderer       = ItemRenderer::new();
        let entity_renderer     = EntityRenderer::new();
        let hotbar_renderer     = HotbarRenderer::new();
        let bag_renderer        = BagRenderer::new();
        let build_renderer      = BuildMenuRenderer::new();
        let mut console         = ConsoleRenderer::new();
        let mut mp_menu         = MultiplayerMenuRenderer::new();
        let mut options_menu    = OptionsMenuRenderer::new();
        let underwater_renderer = UnderwaterRenderer::new();
        let mut load_menu       = LoadMenuRenderer::new();
        let mut load_saves: Vec<String> = Vec::new();
        let mut pending_load: Option<save::SaveData> = None;

        // ── Network state ─────────────────────────────────────────────────────
        let mut net_server: Option<GameServer> = None;
        let mut net_client: Option<GameClient> = None;
        let mut mp_ip = String::from("127.0.0.1");

        // ── Game state ────────────────────────────────────────────────────────
        let mut game_state = GameState::MainMenu;
        let mut menu_yaw: f32 = 0.0; // slowly panning bird's-eye camera
        let mut menu_reveal_timer: f32 = 0.0; // seconds elapsed since chunks finished loading

        let entity_registry = EntityRegistry::load("assets/entities");

        let mut chickens: Vec<Chicken> = Vec::new();
        let mut pigs: Vec<Pig> = Vec::new();
        let mut item_entities: Vec<ItemEntity> = Vec::new();

        let mut hotbar: [Option<(ItemType, u32)>; 9] = [None; 9];
        let mut selected_slot: usize = 0;
        let mut cursor_item: Option<(ItemType, u32)> = None;
        let mut bag_open     = false;
        let mut build_open   = false;
        let mut console_open = false;
        let mut options_open = false;
        let mut fog_distance = FogDistance::Normal;
        let mut chunk_radius_idx: usize = 2; // default radius = CHUNK_RADII[2] = 8
        let mut console_swallow_char = false;
        let mut paused = false;
        let mut loading_menu_timer: f32 = 0.0; // time spent in LoadingMenu state
        let mut outline_enabled = true;
        let mut hi_res = true;
        let mut win_w: f32 = 1600.0;
        let mut win_h: f32 = 1200.0;

        // Camera
        let mut camera = Camera::new(1600, 1200);
        let mut last_mouse_x = 800.0f32;
        let mut last_mouse_y = 600.0f32;
        let mut first_mouse  = true;

        use std::collections::HashMap;
        let mut keys_pressed: HashMap<Key, bool> = HashMap::new();

        let mut wireframe_mode = false;
        let mut last_frame = Instant::now();

        const DAY_LENGTH_SECS: f32 = 300.0;
        let mut sun_angle: f32 = std::f32::consts::FRAC_PI_4;
        let mut total_time: f32 = 0.0;

        // Digging / hit state
        let mut lmb_held = false;
        let mut dig_target: Option<[i32; 3]> = None;
        let mut dig_progress: f32 = 0.0;
        let mut swing_time: f32 = 0.0;
        let mut entity_hit_cooldown: f32 = 0.0;

        // Show OS cursor for the main menu
        window.set_cursor_mode(glfw::CursorMode::Normal);

        // How many chunks we want loaded before the menu "feels" ready.
        // The world generates roughly a 6×6 ring of chunks around spawn, so 36
        // is safely achievable and the bar reaches 100 % without stalling.
        const MENU_CHUNK_TARGET: usize = 150;

        // ── Main loop ─────────────────────────────────────────────────────────
        while !window.should_close() {
            let now = Instant::now();
            let delta_time = now.duration_since(last_frame).as_secs_f32();
            last_frame = now;

            // ── Events ────────────────────────────────────────────────────────
            glfw.poll_events();
            for (_, event) in glfw::flush_messages(&events) {
                match event {

                    glfw::WindowEvent::CursorPos(x, y) => {
                        let xoffset = x as f32 - last_mouse_x;
                        let yoffset = last_mouse_y - y as f32;
                        last_mouse_x = x as f32;
                        last_mouse_y = y as f32;

                        if game_state == GameState::Playing && !paused && !bag_open && !build_open {
                            if first_mouse { first_mouse = false; }
                            else { player.process_mouse_movement(xoffset, yoffset); }
                        }
                    }

                    glfw::WindowEvent::MouseButton(glfw::MouseButton::Button1, Action::Press, _) => {
                        // Gameplay only — capture cursor and start digging.
                        if game_state == GameState::Playing && !paused && !bag_open
                            && !build_open && !options_open
                        {
                            window.set_cursor_mode(glfw::CursorMode::Disabled);
                            lmb_held = true;
                        }
                    }

                    glfw::WindowEvent::MouseButton(glfw::MouseButton::Button1, Action::Release, _) => {
                        lmb_held = false;

                        // All UI button actions fire on release so dragging off a
                        // button before releasing cancels the click (standard behaviour).
                        if options_open {
                            match options_menu.handle_click(last_mouse_x, last_mouse_y, win_w, win_h) {
                                Some("fog")     => fog_distance = fog_distance.next(),
                                Some("chunks")  => {
                                    chunk_radius_idx = (chunk_radius_idx + 1) % CHUNK_RADII.len();
                                    world.set_radius(CHUNK_RADII[chunk_radius_idx]);
                                }
                                Some("outline") => outline_enabled = !outline_enabled,
                                Some("res") => {
                                    hi_res = !hi_res;
                                    let (nw, nh) = if hi_res { (1600, 1200) } else { (800, 600) };
                                    window.set_size(nw, nh);
                                    win_w = nw as f32;
                                    win_h = nh as f32;
                                    camera.on_resize(nw as u32, nh as u32);
                                    gl::Viewport(0, 0, nw, nh);
                                }
                                Some("back") => options_open = false,
                                _ => {}
                            }
                        } else { match game_state {
                            GameState::MainMenu => {
                                let menu_ready = world.chunk_count() >= MENU_CHUNK_TARGET;
                                match main_menu.handle_click(last_mouse_x, last_mouse_y, win_w, win_h, menu_ready) {
                                    Some("options") => options_open = true,
                                    Some("singleplayer") => {
                                        let seed = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default().subsec_nanos();
                                        world = World::new(8, seed);
                                        world.update([8.5, 0.0, 8.5]);
                                        chickens.clear();
                                        pigs.clear();
                                        item_entities.clear();
                                        menu_reveal_timer = 0.0;
                                        game_state = GameState::LoadingGame;
                                    }
                                    Some("load_game") => {
                                        load_saves = save::list_saves();
                                        load_menu.refresh(&load_saves);
                                        game_state = GameState::LoadMenu;
                                    }
                                    Some("multiplayer") => game_state = GameState::MultiplayerMenu,
                                    _ => {}
                                }
                            }
                            GameState::LoadMenu => {
                                match load_menu.handle_click(last_mouse_x, last_mouse_y, win_w, win_h) {
                                    Some("back") => game_state = GameState::MainMenu,
                                    Some(name) => {
                                        match save::load(name) {
                                            Ok(data) => {
                                                world = World::new(8, data.seed);
                                                world.update([8.5, 0.0, 8.5]);
                                                chickens.clear();
                                                pigs.clear();
                                                item_entities.clear();
                                                menu_reveal_timer = 0.0;
                                                pending_load = Some(data);
                                                game_state = GameState::LoadingGame;
                                            }
                                            Err(e) => eprintln!("[save] Failed to load '{}': {}", name, e),
                                        }
                                    }
                                    None => {}
                                }
                            }
                            GameState::MultiplayerMenu => {
                                match mp_menu.handle_click(last_mouse_x, last_mouse_y, win_w, win_h) {
                                    Some("host") => {
                                        match GameServer::new(SERVER_PORT) {
                                            Ok(server) => {
                                                net_server = Some(server);
                                                let seed = std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .unwrap_or_default().subsec_nanos();
                                                world = World::new(8, seed);
                                                world.update([8.5, 0.0, 8.5]);
                                                chickens.clear();
                                                item_entities.clear();
                                                menu_reveal_timer = 0.0;
                                                game_state = GameState::LoadingGame;
                                            }
                                            Err(e) => eprintln!("Failed to start server: {}", e),
                                        }
                                    }
                                    Some("join") => {
                                        mp_menu.join_mode = true;
                                    }
                                    Some("connect") => {
                                        let addr_str = format!("{}:{}", mp_ip, SERVER_PORT);
                                        match addr_str.parse() {
                                            Ok(addr) => match GameClient::connect(addr) {
                                                Ok(client) => {
                                                    net_client = Some(client);
                                                    mp_menu.join_mode = false;
                                                    let seed = std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap_or_default().subsec_nanos();
                                                    world = World::new(8, seed);
                                                    world.update([8.5, 0.0, 8.5]);
                                                    chickens.clear();
                                                    item_entities.clear();
                                                    menu_reveal_timer = 0.0;
                                                    game_state = GameState::LoadingGame;
                                                }
                                                Err(e) => eprintln!("Failed to connect: {}", e),
                                            },
                                            Err(e) => eprintln!("Invalid address: {}", e),
                                        }
                                    }
                                    Some("back") => {
                                        if mp_menu.join_mode {
                                            mp_menu.join_mode = false;
                                        } else {
                                            game_state = GameState::MainMenu;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            GameState::LoadingGame => {}
                            GameState::LoadingMenu => {}
                            GameState::Playing => {
                                if paused {
                                    match menu_renderer.handle_click(last_mouse_x, last_mouse_y, win_w, win_h) {
                                        Some("exit")      => window.set_should_close(true),
                                        Some("options")   => options_open = true,
                                        Some("main_menu") => {
                                            // Tear down the session, then load the panorama world.
                                            paused       = false;
                                            bag_open     = false;
                                            build_open   = false;
                                            console_open = false;
                                            cursor_item  = None;
                                            chickens.clear();
                                            pigs.clear();
                                            item_entities.clear();
                                            player       = Player::new();
                                            hotbar       = [None; 9];
                                            net_server   = None;
                                            net_client   = None;
                                            // Create the new panorama world but do NOT call
                                            // world.update() here — it can block for enough time
                                            // that the next frame's delta_time jumps past the
                                            // 1.5 s loading-screen threshold, skipping the bar.
                                            // The LoadingMenu update loop calls update_loading().
                                            world = World::new(12, 0xDEAD_C0DE);
                                            loading_menu_timer = 0.0;
                                            menu_reveal_timer  = 0.0;
                                            game_state = GameState::LoadingMenu;
                                            window.set_cursor_mode(glfw::CursorMode::Normal);
                                        }
                                        Some("save") => {
                                            let name = save::next_save_name();
                                            let block_changes = world.get_block_changes()
                                                .iter()
                                                .map(|(&[x, y, z], &b)| save::BlockChangeSave {
                                                    x, y, z, block_id: b.to_net_id(),
                                                })
                                                .collect();
                                            let chicken_saves = chickens.iter()
                                                .map(|c| save::EntitySave {
                                                    position: c.position,
                                                    yaw:      c.yaw,
                                                    health:   c.health,
                                                })
                                                .collect();
                                            let pig_saves = pigs.iter()
                                                .map(|p| save::EntitySave {
                                                    position: p.position,
                                                    yaw:      p.yaw,
                                                    health:   p.health,
                                                })
                                                .collect();
                                            let item_saves = item_entities.iter()
                                                .map(|ie| save::ItemSave {
                                                    position: ie.position,
                                                    item_id:  ie.item.tile_index() as u8,
                                                })
                                                .collect();
                                            let inventory_saves: Vec<save::InventorySlotSave> =
                                                player.inventory.iter().enumerate()
                                                .filter_map(|(i, slot)| {
                                                    slot.map(|(item, count)| save::InventorySlotSave {
                                                        index:   i,
                                                        item_id: item.tile_index() as u8,
                                                        count,
                                                    })
                                                })
                                                .collect();
                                            let hotbar_saves: Vec<save::InventorySlotSave> =
                                                hotbar.iter().enumerate()
                                                .filter_map(|(i, slot)| {
                                                    slot.map(|(item, count)| save::InventorySlotSave {
                                                        index:   i,
                                                        item_id: item.tile_index() as u8,
                                                        count,
                                                    })
                                                })
                                                .collect();
                                            let data = save::SaveData {
                                                seed: world.seed(),
                                                sun_angle,
                                                player_position: player.position,
                                                player_yaw: player.yaw,
                                                player_pitch: player.pitch,
                                                block_changes,
                                                chickens: chicken_saves,
                                                pigs: pig_saves,
                                                items: item_saves,
                                                inventory: inventory_saves,
                                                selected_slot,
                                                hotbar: hotbar_saves,
                                            };
                                            if let Err(e) = save::save(&name, &data) {
                                                eprintln!("[save] {}", e);
                                            } else {
                                                println!("[save] Saved as '{}'", name);
                                            }
                                        }
                                        _ => {}
                                    }
                                } else if bag_open {
                                    let nx = last_mouse_x / win_w;
                                    let ny = last_mouse_y / win_h;
                                    if let Some(inv_idx) = bag_renderer.slot_at_pos(nx, ny) {
                                        // Swap cursor ↔ inventory slot (stack if same type)
                                        let slot = &mut player.inventory[inv_idx];
                                        match (&mut cursor_item, slot) {
                                            (Some((ci, cc)), Some((si, sc))) if *ci == *si => {
                                                *sc += *cc;
                                                cursor_item = None;
                                            }
                                            (cursor, slot) => std::mem::swap(cursor, slot),
                                        }
                                    } else if let Some(h) = hotbar_renderer.slot_at_pos(
                                        last_mouse_x, last_mouse_y, win_w, win_h,
                                    ) {
                                        // Swap cursor ↔ hotbar slot (stack if same type)
                                        match (&mut cursor_item, &mut hotbar[h]) {
                                            (Some((ci, cc)), Some((hi, hc))) if *ci == *hi => {
                                                *hc += *cc;
                                                cursor_item = None;
                                            }
                                            (cursor, slot) => std::mem::swap(cursor, slot),
                                        }
                                    }
                                } else if build_open {
                                    build_renderer.handle_click(last_mouse_x, last_mouse_y, win_w, win_h);
                                }
                            }
                        }} // end else { match game_state
                    }

                    glfw::WindowEvent::MouseButton(glfw::MouseButton::Button2, Action::Press, _) => {
                        if game_state == GameState::Playing && !paused && !bag_open && !build_open {
                            let ro = [camera.position.x, camera.position.y, camera.position.z];
                            let rd = [camera.front.x,    camera.front.y,    camera.front.z];
                            if let Some((idx, _)) = nearest_entity_hit(&chickens, ro, rd, 5.0) {
                                chickens[idx].interact();
                            }
                        }
                    }

                    glfw::WindowEvent::Key(Key::F12, _, Action::Press, _) => {
                        wireframe_mode = !wireframe_mode;
                        if wireframe_mode { gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE); }
                        else              { gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL); }
                    }

                    glfw::WindowEvent::Key(key, _, action, modifiers) => {
                        if game_state == GameState::MultiplayerMenu {
                            if action == Action::Press {
                                if key == Key::Escape {
                                    if mp_menu.join_mode {
                                        mp_menu.join_mode = false;
                                    } else {
                                        game_state = GameState::MainMenu;
                                    }
                                } else if key == Key::Backspace && mp_menu.join_mode {
                                    mp_ip.pop();
                                }
                            }
                            continue;
                        }
                        if game_state == GameState::LoadMenu {
                            if key == Key::Escape && action == Action::Press {
                                game_state = GameState::MainMenu;
                            }
                            continue;
                        }
                        if game_state != GameState::Playing {
                            // Escape closes the window from the main menu / loading screen
                            if key == Key::Escape && action == Action::Press {
                                window.set_should_close(true);
                            }
                            continue;
                        }

                        match action {
                            Action::Press => {
                                if key == Key::Escape {
                                    if options_open {
                                        options_open = false;
                                        // paused state unchanged — stay in ESC menu
                                    } else if console_open {
                                        console_open = false;
                                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                                        first_mouse = true;
                                    } else if bag_open {
                                        bag_open = false;
                                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                                    } else if build_open {
                                        build_open = false;
                                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                                    } else {
                                        paused = !paused;
                                        if paused {
                                            window.set_cursor_mode(glfw::CursorMode::Normal);
                                            first_mouse = true;
                                        } else {
                                            window.set_cursor_mode(glfw::CursorMode::Disabled);
                                        }
                                    }
                                } else if console_open && key == Key::Enter {
                                    if let ConsoleAction::Exit = console.submit() {
                                        window.set_should_close(true);
                                    }
                                } else if console_open && key == Key::Backspace {
                                    console.backspace();
                                } else if key == Key::T && !paused && !bag_open && !build_open && !console_open {
                                    console_open = true;
                                    console_swallow_char = true;
                                    window.set_cursor_mode(glfw::CursorMode::Normal);
                                    first_mouse = true;
                                } else if key == Key::I && !paused && !console_open {
                                    bag_open = !bag_open;
                                    build_open = false;
                                    if bag_open {
                                        window.set_cursor_mode(glfw::CursorMode::Normal);
                                        first_mouse = true;
                                    } else {
                                        // Return any held cursor item to inventory on close
                                        if let Some(item) = cursor_item.take() {
                                            if !player.pick_up_stack(item.0, item.1) {
                                                item_entities.push(ItemEntity::new(
                                                    player.position[0], player.position[1] + 1.0,
                                                    player.position[2], item.0,
                                                ));
                                            }
                                        }
                                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                                        first_mouse = true;
                                    }
                                } else if key == Key::B && !paused && !console_open {
                                    build_open = !build_open;
                                    bag_open = false;
                                    if build_open {
                                        window.set_cursor_mode(glfw::CursorMode::Normal);
                                        first_mouse = true;
                                    } else {
                                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                                        first_mouse = true;
                                    }
                                } else if !paused && !console_open {
                                    if key == Key::Space {
                                        player.jump();
                                    } else if let Some(slot) = match key {
                                        Key::Num1 => Some(0), Key::Num2 => Some(1),
                                        Key::Num3 => Some(2), Key::Num4 => Some(3),
                                        Key::Num5 => Some(4), Key::Num6 => Some(5),
                                        Key::Num7 => Some(6), Key::Num8 => Some(7),
                                        Key::Num9 => Some(8), _ => None,
                                    } {
                                        selected_slot = slot;
                                    } else if key == Key::D && modifiers.contains(glfw::Modifiers::Control) {
                                        println!("CTRL+D pressed");
                                    } else {
                                        keys_pressed.insert(key, true);
                                    }
                                }
                            }
                            Action::Release => { keys_pressed.insert(key, false); }
                            _ => {}
                        }
                    }

                    glfw::WindowEvent::Char(c) => {
                        if game_state == GameState::MultiplayerMenu && mp_menu.join_mode {
                            mp_ip.push(c);
                        } else if game_state == GameState::Playing && console_open {
                            if console_swallow_char { console_swallow_char = false; }
                            else { console.type_char(c); }
                        }
                    }

                    _ => {}
                }
            }

            // ── State-specific updates ─────────────────────────────────────────
            match game_state {
                GameState::MainMenu => {
                    world.update([8.0, 28.0, 8.0]);
                }

                GameState::LoadingMenu => {
                    // Load the panorama world fast in the background.
                    world.update_loading([8.0, 28.0, 8.0]);
                    // Cap the per-frame contribution so that a single slow frame
                    // (e.g. from World::new blocking on the previous frame) cannot
                    // jump the timer past the threshold and skip the loading screen.
                    loading_menu_timer += delta_time.min(0.05);
                    // After 1.5 s the bar is full; skip straight to a revealed menu.
                    if loading_menu_timer >= 1.5 {
                        menu_reveal_timer = 2.0; // menu_revealed=true → panorama shows at once
                        game_state = GameState::MainMenu;
                    }
                }

                GameState::LoadMenu => {
                    world.update([8.0, 28.0, 8.0]);
                }

                GameState::MultiplayerMenu => {
                    world.update([8.0, 28.0, 8.0]);
                    mp_menu.update_ip(&mp_ip);
                }

                GameState::LoadingGame => {
                    // Pump the async pipeline as fast as possible each frame.
                    for _ in 0..8 { world.update_loading([8.5, 0.0, 8.5]); }

                    // Wait until the 3×3 chunk area around spawn is fully meshed.
                    let mut spawn_ready = true;
                    for dx in -1..=1i32 { for dz in -1..=1i32 {
                        if !world.is_chunk_meshed(dx, dz) { spawn_ready = false; }
                    }}
                    if spawn_ready {
                        player.position = world.find_spawn_point();
                        player.velocity = [0.0, 0.0, 0.0];

                        if let Some(data) = pending_load.take() {
                            // ── Load from save ─────────────────────────────
                            player.position = data.player_position;
                            player.yaw      = data.player_yaw;
                            player.pitch    = data.player_pitch;
                            sun_angle       = data.sun_angle;

                            // Re-apply player block modifications
                            world.load_block_changes(data.block_changes_as_map());

                            // Restore entities
                            for es in &data.chickens {
                                if let Some(def) = entity_registry.get("chicken") {
                                    let mut c = Chicken::new(
                                        es.position[0], es.position[1], es.position[2], def,
                                    );
                                    c.yaw    = es.yaw;
                                    c.health = es.health;
                                    chickens.push(c);
                                }
                            }
                            for es in &data.pigs {
                                if let Some(def) = entity_registry.get("pig") {
                                    let mut p = Pig::new(
                                        es.position[0], es.position[1], es.position[2], def,
                                    );
                                    p.yaw    = es.yaw;
                                    p.health = es.health;
                                    pigs.push(p);
                                }
                            }
                            for is in &data.items {
                                if let Some(item) = world::ItemType::from_tile_index(is.item_id as usize) {
                                    item_entities.push(ItemEntity::new(
                                        is.position[0], is.position[1], is.position[2], item,
                                    ));
                                }
                            }
                            player.inventory = [None; game::INVENTORY_SIZE];
                            for slot in &data.inventory {
                                if slot.index < game::INVENTORY_SIZE {
                                    if let Some(item) = world::ItemType::from_tile_index(slot.item_id as usize) {
                                        player.inventory[slot.index] = Some((item, slot.count));
                                    }
                                }
                            }
                            selected_slot = data.selected_slot.min(8);
                            hotbar = [None; 9];
                            for slot in &data.hotbar {
                                if slot.index < 9 {
                                    if let Some(item) = world::ItemType::from_tile_index(slot.item_id as usize) {
                                        hotbar[slot.index] = Some((item, slot.count));
                                    }
                                }
                            }
                        } else {
                            // ── Fresh game — deterministic entity spawn ────
                            let scan_radius: i32 = 12;
                            for cx in -scan_radius..=scan_radius {
                                for cz in -scan_radius..=scan_radius {
                                    let h = (cx.wrapping_mul(73_856_093i32)
                                        ^ cz.wrapping_mul(19_349_663i32)) as u32;
                                    if h % 20 != 0 { continue; }
                                    let center_wx = (cx * 16 + 8) as f64;
                                    let center_wz = (cz * 16 + 8) as f64;
                                    if !world::biome::biome_at_world(center_wx, center_wz)
                                        .allows_chickens() { continue; }
                                    let family_size = 1 + (h >> 8) % 3;
                                    let base_bx = cx * 16 + ((h >> 4) & 0xF) as i32;
                                    let base_bz = cz * 16 + ((h >> 12) & 0xF) as i32;
                                    for i in 0..family_size {
                                        let bx = base_bx + (i as i32 % 2) * 3;
                                        let bz = base_bz + (i as i32 / 2) * 3;
                                        let sy = world.surface_height(bx, bz);
                                        if sy > 10 {
                                            if let Some(def) = entity_registry.get("chicken") {
                                                chickens.push(Chicken::new(
                                                    bx as f32 + 0.5, sy as f32, bz as f32 + 0.5, def,
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                            for cx in -scan_radius..=scan_radius {
                                for cz in -scan_radius..=scan_radius {
                                    let h = (cx.wrapping_mul(48_271i32)
                                        ^ cz.wrapping_mul(83_492_791i32)) as u32;
                                    if h % 25 != 0 { continue; }
                                    let center_wx = (cx * 16 + 8) as f64;
                                    let center_wz = (cz * 16 + 8) as f64;
                                    if !world::biome::biome_at_world(center_wx, center_wz)
                                        .allows_pigs() { continue; }
                                    let family_size = 2 + (h >> 8) % 3;
                                    let base_bx = cx * 16 + ((h >> 6) & 0xF) as i32;
                                    let base_bz = cz * 16 + ((h >> 14) & 0xF) as i32;
                                    for i in 0..family_size {
                                        let bx = base_bx + (i as i32 % 2) * 3;
                                        let bz = base_bz + (i as i32 / 2) * 3;
                                        let sy = world.surface_height(bx, bz);
                                        if sy > 10 {
                                            if let Some(def) = entity_registry.get("pig") {
                                                pigs.push(Pig::new(
                                                    bx as f32 + 0.5, sy as f32, bz as f32 + 0.5, def,
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        world.update(player.position); // seed chunk gen at actual spawn
                        minimap.update(&world, player.position[0], player.position[2]);
                        game_state = GameState::Playing;
                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                        first_mouse = true;
                    }
                }

                GameState::Playing => {
                    if !paused && !bag_open && !build_open && !console_open {
                        // Player movement
                        player.walk(
                            *keys_pressed.get(&Key::W).unwrap_or(&false),
                            *keys_pressed.get(&Key::S).unwrap_or(&false),
                            *keys_pressed.get(&Key::A).unwrap_or(&false),
                            *keys_pressed.get(&Key::D).unwrap_or(&false),
                            *keys_pressed.get(&Key::LeftShift).unwrap_or(&false),
                        );
                        player.apply_physics(delta_time, |x, y, z| world.get_block(x, y, z).is_solid());

                        // Camera follows player
                        camera.update_pitch_yaw(player.pitch, player.yaw);
                        camera.move_to_abs(
                            player.position[0], player.position[1] + 1.6, player.position[2],
                        );

                        let cam_pos = [camera.position.x, camera.position.y, camera.position.z];
                        let cam_dir = [camera.front.x,    camera.front.y,    camera.front.z];

                        entity_hit_cooldown = (entity_hit_cooldown - delta_time).max(0.0);

                        if !lmb_held {
                            if let Some(target) = dig_target {
                                let block = world.get_block(target[0], target[1], target[2]);
                                if let Some(hardness) = block.hardness() {
                                    dig_progress -= (hardness / 30.0) * delta_time;
                                    if dig_progress <= 0.0 {
                                        dig_progress = 0.0;
                                        dig_target = None;
                                    }
                                } else {
                                    dig_target   = None;
                                    dig_progress = 0.0;
                                }
                            }
                        }

                        if lmb_held {
                            let chk_hit = nearest_entity_hit(&chickens, cam_pos, cam_dir, 5.0);
                            let pig_hit = nearest_entity_hit(&pigs,     cam_pos, cam_dir, 5.0);
                            // Pick whichever entity type is closer
                            let (ent_hit_t, hit_chicken, hit_pig_idx) = match (chk_hit, pig_hit) {
                                (Some((ci, ct)), Some((pi, pt))) => if ct <= pt {
                                    (ct, Some(ci), None)
                                } else {
                                    (pt, None, Some(pi))
                                },
                                (Some((ci, ct)), None) => (ct, Some(ci), None),
                                (None, Some((pi, pt))) => (pt, None, Some(pi)),
                                (None, None)           => (f32::MAX, None, None),
                            };
                            let blk_hit = world.raycast(cam_pos, cam_dir, 5.0);

                            let hit_entity = if hit_chicken.is_some() || hit_pig_idx.is_some() {
                                match blk_hit {
                                    Some(b) => {
                                        let dx = b[0] as f32 + 0.5 - cam_pos[0];
                                        let dy = b[1] as f32 + 0.5 - cam_pos[1];
                                        let dz = b[2] as f32 + 0.5 - cam_pos[2];
                                        ent_hit_t < (dx*dx + dy*dy + dz*dz).sqrt()
                                    }
                                    None => true,
                                }
                            } else { false };

                            if hit_entity {
                                if entity_hit_cooldown <= 0.0 {
                                    let len = (cam_dir[0]*cam_dir[0] + cam_dir[2]*cam_dir[2])
                                        .sqrt().max(0.001);
                                    let push = [cam_dir[0]/len, 0.0, cam_dir[2]/len];
                                    if let Some(ci) = hit_chicken { chickens[ci].take_hit(push); }
                                    if let Some(pi) = hit_pig_idx { pigs[pi].take_hit(push); }
                                    entity_hit_cooldown = 0.4;
                                }
                                dig_target   = None;
                                dig_progress = 0.0;
                            } else if let Some(target) = blk_hit {
                                let block = world.get_block(target[0], target[1], target[2]);
                                if Some(target) != dig_target {
                                    dig_target   = Some(target);
                                    dig_progress = 0.0;
                                }
                                if let Some(hardness) = block.hardness() {
                                    dig_progress += delta_time;
                                    if dig_progress >= hardness {
                                        let drops = block.drops(target[0], target[1], target[2]);
                                        world.set_block_recorded(target[0], target[1], target[2],
                                            world::BlockType::Air);
                                        if let Some(ref mut server) = net_server {
                                            server.broadcast_block_change(
                                                target[0], target[1], target[2], 0,
                                            );
                                        }
                                        if let Some(ref mut client) = net_client {
                                            client.send_block_break(
                                                target[0], target[1], target[2],
                                            );
                                        }
                                        for item_type in drops {
                                            item_entities.push(ItemEntity::new(
                                                target[0] as f32, target[1] as f32,
                                                target[2] as f32, item_type,
                                            ));
                                        }
                                        dig_target   = None;
                                        dig_progress = 0.0;
                                    }
                                }
                            } else {
                                dig_target   = None;
                                dig_progress = 0.0;
                            }
                        }

                        swing_time = if lmb_held { swing_time + delta_time } else { 0.0 };

                        for entity in &mut item_entities {
                            entity.update(delta_time, |x, y, z| world.get_block(x, y, z));
                        }
                        for chicken in &mut chickens {
                            chicken.update(delta_time, |x, y, z| world.get_block(x, y, z));
                        }
                        for pig in &mut pigs {
                            pig.update(delta_time, |x, y, z| world.get_block(x, y, z));
                        }
                        for chicken in chickens.iter().filter(|c| c.is_dead()) {
                            for item_type in chicken.drops() {
                                item_entities.push(ItemEntity::new(
                                    chicken.position[0], chicken.position[1] + 0.5,
                                    chicken.position[2], item_type,
                                ));
                            }
                        }
                        chickens.retain(|c| !c.is_dead());
                        for pig in pigs.iter().filter(|p| p.is_dead()) {
                            for item_type in pig.drops() {
                                item_entities.push(ItemEntity::new(
                                    pig.position[0], pig.position[1] + 0.5,
                                    pig.position[2], item_type,
                                ));
                            }
                        }
                        pigs.retain(|p| !p.is_dead());

                        item_entities.retain(|entity| {
                            let dx = entity.position[0] + 0.5 - player.position[0];
                            let dy = entity.position[1] + 0.5 - (player.position[1] + 0.9);
                            let dz = entity.position[2] + 0.5 - player.position[2];
                            if (dx*dx + dy*dy + dz*dz).sqrt() < 1.5 {
                                !player.pick_up(entity.item)
                            } else { true }
                        });

                        world.update(player.position);
                        world.tick_water(delta_time);
                    }

                    // ── Network tick ──────────────────────────────────────────
                    if let Some(ref mut server) = net_server {
                        server.update(delta_time);
                        for [x, y, z] in server.drain_block_breaks() {
                            world.set_block(x, y, z, world::BlockType::Air);
                        }
                        server.broadcast_host_position(
                            player.position[0], player.position[1], player.position[2],
                            player.yaw, player.pitch,
                        );
                    }
                    if let Some(ref mut client) = net_client {
                        for msg in client.update(delta_time) {
                            if let ServerMessage::BlockChange { x, y, z, block_id } = msg {
                                world.set_block(x, y, z, world::BlockType::from_net_id(block_id));
                            }
                        }
                        client.send_position(
                            player.position[0], player.position[1], player.position[2],
                            player.yaw, player.pitch,
                        );
                    }
                }
            }

            // ── Sun / sky (common to all states) ──────────────────────────────
            total_time += delta_time;
            sun_angle += delta_time * (std::f32::consts::TAU / DAY_LENGTH_SECS);
            if sun_angle > std::f32::consts::TAU { sun_angle -= std::f32::consts::TAU; }

            let sun_pos  = glam::Vec3::new(sun_angle.cos(), sun_angle.sin(), 0.3);
            let moon_pos = -sun_pos;
            let light_pos = if sun_pos.y >= 0.0 { sun_pos } else { moon_pos };
            let sun_dir   = (-light_pos).normalize();

            let smoothstep = |a: f32, b: f32, x: f32| {
                let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let altitude = sun_pos.y;
            let day_w    = smoothstep(0.0, 0.25,  altitude);
            let night_w  = smoothstep(0.0, 0.25, -altitude);
            let dusk_w   = (1.0 - day_w - night_w).max(0.0);

            let day_blue    = glam::Vec3::new(0.53, 0.81, 0.92);
            let dusk_orange = glam::Vec3::new(1.00, 0.50, 0.30);
            let night_dark  = glam::Vec3::new(0.04, 0.05, 0.12);
            let sky_color   = day_blue * day_w + dusk_orange * dusk_w + night_dark * night_w;

            let ambient_light    = 0.45 * day_w + 0.25 * dusk_w + 0.10 * night_w;
            let active_alt       = altitude.abs();
            let dir_t            = smoothstep(0.0, 0.20, active_alt);
            let dir_max          = if altitude >= 0.0 { 0.55 } else { 0.15 };
            let directional_light = dir_max * dir_t;

            // ── Camera (menu panorama only — not during game loading) ─────────
            if matches!(game_state, GameState::MainMenu | GameState::LoadMenu | GameState::MultiplayerMenu) {
                menu_yaw += delta_time * 0.006;
                camera.position = glam::Vec3::new(8.0, 16.0, 8.0);
                camera.front    = glam::Vec3::new(
                    menu_yaw.sin(), -0.06, menu_yaw.cos(),
                ).normalize();
                camera.up = glam::Vec3::new(0.0, 1.0, 0.0);
            }

            let view       = camera.view_matrix();
            let projection = camera.projection_matrix();

            let menu_ready = world.chunk_count() >= MENU_CHUNK_TARGET;
            if menu_ready && matches!(game_state, GameState::MainMenu | GameState::LoadMenu | GameState::MultiplayerMenu) {
                menu_reveal_timer += delta_time;
            }
            let menu_revealed = menu_reveal_timer >= 2.0;
            let show_3d = game_state == GameState::Playing || menu_revealed;

            // ── 3D render ─────────────────────────────────────────────────────
            gl::ClearColor(sky_color.x, sky_color.y, sky_color.z, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

            let (fb_w, fb_h) = window.get_framebuffer_size();
            let (fog_start, fog_end) = fog_distance.fog_params();

            if show_3d {
                sky_renderer.draw(&view, &projection, sky_color, 0.5 + 0.8 * day_w);
                shadow_pass.begin(
                    sun_dir,
                    camera.position, camera.front, camera.up,
                    camera.fov.to_radians(), camera.aspect_ratio,
                    chunk_renderer.texture_atlas(),
                );
                for i in 0..NUM_CASCADES {
                    shadow_pass.begin_cascade(i);
                    world.draw_shadow(&shadow_pass);
                    if game_state == GameState::Playing {
                        entity_renderer.draw_shadows(&chickens, &shadow_pass);
                        entity_renderer.draw_pig_shadows(&pigs, &shadow_pass);
                    }
                }
                shadow_pass.end(fb_w, fb_h);

                if sun_pos.y > 0.0 {
                    let pale = glam::Vec3::new(1.0, 0.95, 0.80);
                    let warm = glam::Vec3::new(1.0, 0.55, 0.30);
                    let t = smoothstep(0.0, 0.25, sun_pos.y);
                    let color = warm + (pale - warm) * t;
                    sun_renderer.draw(&view, &projection, camera.position,
                        sun_pos.normalize(), 25.0, color);
                }
                if moon_pos.y > 0.0 {
                    sun_renderer.draw(&view, &projection, camera.position,
                        moon_pos.normalize(), 18.0, glam::Vec3::new(0.85, 0.88, 1.0));
                }

                chunk_renderer.begin_frame(
                    &view, &projection,
                    shadow_pass.depth_texture_array(),
                    shadow_pass.light_space_matrices(),
                    &CASCADE_ENDS,
                    shadow_pass.texel_world_sizes(),
                    sun_dir, sky_color, ambient_light, directional_light,
                    fog_start, fog_end,
                    total_time,
                    camera.position,
                    camera.near_plane,
                    camera.far_plane,
                    fb_w, fb_h,
                    11.0, // world Y of water surface (SEA_LEVEL=10 + 1 block height)
                );
                chunk_renderer.capture_sky(fb_w, fb_h);
                world.draw_opaque(&chunk_renderer, &camera);

                // ── Draw world entities before the water pass ──────────────────
                // This ensures the water surface correctly blends over submerged
                // portions of entities via depth testing + alpha blending.
                if game_state == GameState::Playing {
                    let sky_tex = chunk_renderer.sky_texture();
                    item_renderer.draw(&item_entities, &view, &projection);
                    entity_renderer.draw_chickens(&chickens, &view, &projection,
                        fog_start, fog_end, fb_w as f32, fb_h as f32, sky_tex);
                    entity_renderer.draw_pigs(&pigs, &view, &projection,
                        fog_start, fog_end, fb_w as f32, fb_h as f32, sky_tex);
                    let remote_peers: Vec<([f32; 3], f32)> = if let Some(ref server) = net_server {
                        server.remote_players()
                    } else if let Some(ref client) = net_client {
                        client.remote_players()
                    } else { vec![] };
                    for (pos, yaw) in remote_peers {
                        player_renderer.draw(pos, yaw, &view, &projection, PlayerDrawMode::Full, 0.0,
                            fog_start, fog_end, fb_w as f32, fb_h as f32, sky_tex);
                    }
                }

                // Capture scene (terrain + entities) for water refraction, then draw water
                chunk_renderer.capture_scene(fb_w, fb_h);
                world.draw_transparent(&chunk_renderer, &camera);
                chunk_renderer.end_frame();
            }

            // ── Playing-only overlays (drawn after water) ──────────────────────
            if game_state == GameState::Playing {
                let sky_tex = chunk_renderer.sky_texture();
                const SWING_SPEED: f32 = 8.0;
                const SWING_AMP_BASE: f32 = 1.4;
                let swing_angle = if lmb_held {
                    let pitch_rad = player.pitch.to_radians();
                    let target = (SWING_AMP_BASE + pitch_rad).clamp(0.1, std::f32::consts::PI * 0.85);
                    (swing_time * SWING_SPEED).sin().abs() * target
                } else { 0.0 };
                player_renderer.draw(player.position, player.yaw, &view, &projection,
                    PlayerDrawMode::ArmsOnly, swing_angle,
                    fog_start, fog_end, fb_w as f32, fb_h as f32, sky_tex);

                if outline_enabled && !paused && !bag_open && !build_open {
                    let ro  = [camera.position.x, camera.position.y, camera.position.z];
                    let rd  = [camera.front.x,    camera.front.y,    camera.front.z];
                    let chk_dist = nearest_entity_hit(&chickens, ro, rd, 5.0).map(|(_, t)| t);
                    let pig_dist = nearest_entity_hit(&pigs,     ro, rd, 5.0).map(|(_, t)| t);
                    let ent_dist = match (chk_dist, pig_dist) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (a, b) => a.or(b),
                    };
                    if let Some(block) = world.raycast(ro, rd, 5.0) {
                        let dx = block[0] as f32 + 0.5 - ro[0];
                        let dy = block[1] as f32 + 0.5 - ro[1];
                        let dz = block[2] as f32 + 0.5 - ro[2];
                        let blk_dist = (dx*dx + dy*dy + dz*dz).sqrt();
                        if ent_dist.map_or(true, |ed| blk_dist <= ed) {
                            outline_renderer.draw(block, &view, &projection);
                        }
                    }
                }

                if lmb_held {
                    if let Some(target) = dig_target {
                        let block = world.get_block(target[0], target[1], target[2]);
                        if let Some(hardness) = block.hardness() {
                            let stage = ((dig_progress / hardness * 5.0) as usize).min(4);
                            crack_renderer.draw(target, stage, &view, &projection);
                        }
                    }
                }

                // ── HUD ───────────────────────────────────────────────────────
                minimap.update(&world, player.position[0], player.position[2]);

                let eye = camera.position;
                if world.get_block(eye.x.floor() as i32, eye.y.floor() as i32, eye.z.floor() as i32)
                    == world::BlockType::Water
                {
                    underwater_renderer.draw(total_time, win_w, win_h);
                }

                crosshair_renderer.draw();
                health_bar.draw(player.health as f32 / 100.0);
                hotbar_renderer.draw(selected_slot, &hotbar, win_w, win_h);

                let time_of_day = (sun_angle / std::f32::consts::TAU * 24.0 + 6.0).rem_euclid(24.0);
                let tod_hours   = time_of_day as u32;
                let tod_minutes = (time_of_day.fract() * 60.0) as u32;
                let tod_seconds = (time_of_day.fract() * 3600.0) as u32 % 60;
                clock_renderer.draw(tod_hours, tod_minutes, tod_seconds, win_w, win_h);

                let minimap_ent_pos: Vec<(f32, f32)> = chickens.iter()
                    .map(|c| (c.position[0], c.position[2])).collect();
                let minimap_ent_col: Vec<(f32, f32, f32)> = chickens.iter()
                    .map(|_| (1.0f32, 0.55, 0.0)).collect();
                minimap.draw(player.position[0], player.position[2],
                    camera.front.x, camera.front.z,
                    &minimap_ent_pos, &minimap_ent_col, win_w, win_h);

                if bag_open {
                    bag_renderer.draw(&player.inventory, None);
                    if let Some((item, count)) = cursor_item {
                        bag_renderer.draw_cursor_item(item, count,
                            last_mouse_x / win_w, last_mouse_y / win_h);
                    }
                }
                if build_open {
                    build_renderer.draw(last_mouse_x / win_w, last_mouse_y / win_h);
                }
                if paused && !options_open { menu_renderer.draw(win_w, win_h); }
                if console_open { console.draw(win_w, win_h); }
            }

            // ── Menu / loading UI ──────────────────────────────────────────────
            match game_state {
                GameState::MainMenu => {
                    if menu_revealed {
                        main_menu.draw(1.0, true, win_w, win_h);
                    } else {
                        let progress = (world.chunk_count() as f32 / MENU_CHUNK_TARGET as f32).min(1.0);
                        main_menu.draw_loading_screen(progress, win_w, win_h);
                    }
                }
                GameState::LoadMenu => {
                    load_menu.draw(win_w, win_h);
                }
                GameState::MultiplayerMenu => {
                    if menu_revealed {
                        main_menu.draw(1.0, true, win_w, win_h);
                    } else {
                        let progress = (world.chunk_count() as f32 / MENU_CHUNK_TARGET as f32).min(1.0);
                        main_menu.draw_loading_screen(progress, win_w, win_h);
                    }
                    mp_menu.draw(win_w, win_h);
                }
                GameState::LoadingGame => {
                    // Progress = how many of the 3×3 spawn chunks are meshed.
                    let mut meshed = 0usize;
                    for dx in -1..=1i32 { for dz in -1..=1i32 {
                        if world.is_chunk_meshed(dx, dz) { meshed += 1; }
                    }}
                    let progress = meshed as f32 / 9.0;
                    main_menu.draw_loading_screen(progress, win_w, win_h);
                }
                GameState::LoadingMenu => {
                    // Timer-based: 0 → 1 over 1.5 s, guaranteed smooth animation.
                    let progress = (loading_menu_timer / 1.5).min(1.0);
                    main_menu.draw_loading_screen(progress, win_w, win_h);
                }
                GameState::Playing => {}
            }

            // Options menu overlays on top of everything else.
            if options_open {
                options_menu.draw(fog_distance.as_idx(), chunk_radius_idx, outline_enabled, hi_res, win_w, win_h);
            }

            window.swap_buffers();
        }

        drop(chunk_renderer);
    }
}
