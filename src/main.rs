use glfw::{Action, Context, Key};
use std::os::raw::c_void;
use std::ptr;
use std::time::Instant;

mod game;
use game::Player;

mod camera;
use camera::Camera;

mod world;
use world::{World, ItemEntity, ItemType, Chicken, nearest_entity_hit};

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

mod net;
use net::{GameServer, GameClient};
use net::messages::{ServerMessage, SERVER_PORT};

#[derive(PartialEq, Eq)]
enum GameState {
    MainMenu,
    MultiplayerMenu,
    LoadingGame,
    Playing,
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
        let mut world  = World::new(4);
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

        // ── Network state ─────────────────────────────────────────────────────
        let mut net_server: Option<GameServer> = None;
        let mut net_client: Option<GameClient> = None;
        let mut mp_ip = String::from("127.0.0.1");

        // ── Game state ────────────────────────────────────────────────────────
        let mut game_state = GameState::MainMenu;
        let mut menu_yaw: f32 = 0.0; // slowly panning bird's-eye camera

        let mut chickens: Vec<Chicken> = Vec::new();
        let mut item_entities: Vec<ItemEntity> = Vec::new();

        let hotbar: [Option<ItemType>; 9] = [None; 9];
        let mut selected_slot: usize = 0;
        let mut bag_open     = false;
        let mut build_open   = false;
        let mut console_open = false;
        let mut console_swallow_char = false;
        let mut paused = false;
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
        const MENU_CHUNK_TARGET: usize = 36;

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
                        match game_state {
                            GameState::MainMenu => {
                                let menu_ready = world.chunk_count() >= MENU_CHUNK_TARGET;
                                match main_menu.handle_click(last_mouse_x, last_mouse_y, win_w, win_h, menu_ready) {
                                    Some("singleplayer") => {
                                        game_state = GameState::LoadingGame;
                                    }
                                    Some("multiplayer") => game_state = GameState::MultiplayerMenu,
                                    _ => {}
                                }
                            }
                            GameState::MultiplayerMenu => {
                                match mp_menu.handle_click(last_mouse_x, last_mouse_y, win_w, win_h) {
                                    Some("host") => {
                                        match GameServer::new(SERVER_PORT) {
                                            Ok(server) => {
                                                net_server = Some(server);
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
                            GameState::Playing => {
                                if paused {
                                    match menu_renderer.handle_click(last_mouse_x, last_mouse_y, win_w, win_h) {
                                        Some("exit") => window.set_should_close(true),
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
                                        _ => {}
                                    }
                                } else if build_open {
                                    build_renderer.handle_click(last_mouse_x, last_mouse_y, win_w, win_h);
                                } else if bag_open {
                                    // keep cursor visible; bag UI handles its own logic
                                } else {
                                    window.set_cursor_mode(glfw::CursorMode::Disabled);
                                    lmb_held = true;
                                }
                            }
                        }
                    }

                    glfw::WindowEvent::MouseButton(glfw::MouseButton::Button1, Action::Release, _) => {
                        lmb_held = false;
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
                                    if console_open {
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
                    // Let the world generate in the background for the scenic view.
                    world.finalize_all_pending();
                    world.update([8.0, 28.0, 8.0]);
                }

                GameState::MultiplayerMenu => {
                    world.finalize_all_pending();
                    world.update([8.0, 28.0, 8.0]);
                    mp_menu.update_ip(&mp_ip);
                }

                GameState::LoadingGame => {
                    // A handful of finalizations per frame — enough to make
                    // progress without freezing the render loop.
                    for _ in 0..5 { world.finalize_all_pending(); }
                    world.update([8.5, 0.0, 8.5]);

                    // Wait for the spawn chunk centre (8, 8) — safely inside
                    // chunk (0,0) so no 4-chunk corner ambiguity.
                    if world.surface_height(8, 8) > 0 {
                        // Spawn a couple of blocks above the computed surface so the
                        // player is guaranteed to be in open air — physics then drops
                        // them cleanly onto the surface, just like the old fall-from-y64 approach.
                        let spawn_y = world.surface_height(8, 8) + 2;
                        player.position  = [8.5, spawn_y as f32, 8.5];
                        player.velocity  = [0.0, 0.0, 0.0]; // clear any accumulated fall

                        // Spawn chickens wherever heights are currently known
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
                                        chickens.push(Chicken::new(
                                            bx as f32 + 0.5, sy as f32, bz as f32 + 0.5,
                                        ));
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
                            let ent_hit = nearest_entity_hit(&chickens, cam_pos, cam_dir, 5.0);
                            let blk_hit = world.raycast(cam_pos, cam_dir, 5.0);

                            let hit_entity = match (ent_hit, blk_hit) {
                                (Some((_, et)), Some(b)) => {
                                    let dx = b[0] as f32 + 0.5 - cam_pos[0];
                                    let dy = b[1] as f32 + 0.5 - cam_pos[1];
                                    let dz = b[2] as f32 + 0.5 - cam_pos[2];
                                    et < (dx*dx + dy*dy + dz*dz).sqrt()
                                }
                                (Some(_), None) => true,
                                _ => false,
                            };

                            if hit_entity {
                                if entity_hit_cooldown <= 0.0 {
                                    let (idx, _) = ent_hit.unwrap();
                                    let len = (cam_dir[0]*cam_dir[0] + cam_dir[2]*cam_dir[2])
                                        .sqrt().max(0.001);
                                    chickens[idx].take_hit([cam_dir[0]/len, 0.0, cam_dir[2]/len]);
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
                                        world.set_block(target[0], target[1], target[2],
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
                        for chicken in chickens.iter().filter(|c| c.is_dead()) {
                            for item_type in chicken.drops() {
                                item_entities.push(ItemEntity::new(
                                    chicken.position[0], chicken.position[1] + 0.5,
                                    chicken.position[2], item_type,
                                ));
                            }
                        }
                        chickens.retain(|c| !c.is_dead());

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

            // ── Camera (override for non-Playing states) ───────────────────────
            if game_state != GameState::Playing {
                // Slow-panning bird's-eye at ~10 player-heights above the terrain
                menu_yaw += delta_time * 0.15;
                camera.position = glam::Vec3::new(8.0, 35.0, 8.0);
                camera.front    = glam::Vec3::new(
                    menu_yaw.sin() * 0.75, -0.55, menu_yaw.cos() * 0.75,
                ).normalize();
                camera.up = glam::Vec3::new(0.0, 1.0, 0.0);
            }

            let view       = camera.view_matrix();
            let projection = camera.projection_matrix();

            // ── 3D render (always — forms the menu background too) ─────────────
            gl::ClearColor(sky_color.x, sky_color.y, sky_color.z, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

            sky_renderer.draw(&view, &projection, sky_color, 0.5 + 0.8 * day_w);

            let (fb_w, fb_h) = window.get_framebuffer_size();
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
            );
            world.draw(&chunk_renderer, &camera);
            chunk_renderer.end_frame();

            // ── Playing-only 3D objects ────────────────────────────────────────
            if game_state == GameState::Playing {
                const SWING_SPEED: f32 = 8.0;
                const SWING_AMP_BASE: f32 = 1.4;
                let swing_angle = if lmb_held {
                    let pitch_rad = player.pitch.to_radians();
                    let target = (SWING_AMP_BASE + pitch_rad).clamp(0.1, std::f32::consts::PI * 0.85);
                    (swing_time * SWING_SPEED).sin().abs() * target
                } else { 0.0 };
                player_renderer.draw(player.position, player.yaw, &view, &projection,
                    PlayerDrawMode::ArmsOnly, swing_angle);

                if outline_enabled && !paused && !bag_open && !build_open {
                    let ro  = [camera.position.x, camera.position.y, camera.position.z];
                    let rd  = [camera.front.x,    camera.front.y,    camera.front.z];
                    let ent_dist = nearest_entity_hit(&chickens, ro, rd, 5.0).map(|(_, t)| t);
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

                item_renderer.draw(&item_entities, &view, &projection);
                entity_renderer.draw_chickens(&chickens, &view, &projection);

                // Render remote peers
                let remote_peers: Vec<([f32; 3], f32)> = if let Some(ref server) = net_server {
                    server.remote_players()
                } else if let Some(ref client) = net_client {
                    client.remote_players()
                } else {
                    vec![]
                };
                for (pos, yaw) in remote_peers {
                    player_renderer.draw(pos, yaw, &view, &projection, PlayerDrawMode::Full, 0.0);
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
                    hotbar_renderer.draw_fullscreen_tint([0.05, 0.20, 0.60, 0.35], win_w, win_h);
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

                if bag_open   { bag_renderer.draw(&player.inventory); }
                if build_open {
                    build_renderer.draw(last_mouse_x / win_w, last_mouse_y / win_h);
                }
                if paused     { menu_renderer.draw(outline_enabled, hi_res, win_w, win_h); }
                if console_open { console.draw(win_w, win_h); }
            }

            // ── Menu / loading UI ──────────────────────────────────────────────
            match game_state {
                GameState::MainMenu => {
                    let loaded = world.chunk_count().min(MENU_CHUNK_TARGET);
                    let progress = loaded as f32 / MENU_CHUNK_TARGET as f32;
                    main_menu.draw(progress, loaded >= MENU_CHUNK_TARGET, win_w, win_h);
                }
                GameState::MultiplayerMenu => {
                    let loaded = world.chunk_count().min(MENU_CHUNK_TARGET);
                    main_menu.draw(
                        loaded as f32 / MENU_CHUNK_TARGET as f32,
                        loaded >= MENU_CHUNK_TARGET,
                        win_w, win_h,
                    );
                    mp_menu.draw(win_w, win_h);
                }
                GameState::LoadingGame => {
                    // Progress: 9 sample points centred on chunk (0,0) interior
                    let checks: &[(i32, i32)] = &[
                        (8,8),(24,8),(-8,8),(8,24),(8,-8),(24,24),(-8,24),(24,-8),(-8,-8),
                    ];
                    let loaded = checks.iter()
                        .filter(|&&(x, z)| world.surface_height(x, z) > 0)
                        .count();
                    let progress = loaded as f32 / checks.len() as f32;
                    main_menu.draw_loading_screen(progress, win_w, win_h);
                }
                GameState::Playing => {}
            }

            window.swap_buffers();
        }

        drop(chunk_renderer);
    }
}
