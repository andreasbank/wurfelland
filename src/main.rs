use glfw::{Action, Context, Key};
use std::os::raw::c_void;
use std::ptr;
use std::time::Instant;

mod game;
use game::Player;

mod camera;
use camera::Camera;

mod world;
use world::{World, ItemEntity, ItemType};

mod renderer;
use renderer::ChunkRenderer;
use renderer::crosshair_renderer;
use renderer::HealthBar;
use renderer::MenuRenderer;
use renderer::BlockOutlineRenderer;
use renderer::PlayerRenderer;
use renderer::player_renderer::PlayerDrawMode;
use renderer::CrackRenderer;
use renderer::ItemRenderer;
use renderer::HotbarRenderer;
use renderer::BagRenderer;


fn main() {
    // Initialize GLFW
    let mut glfw = glfw::init_no_callbacks().unwrap();
    
    // Create window
    let (mut window, events) = glfw
        .create_window(1600, 1200, "Wurfelland", glfw::WindowMode::Windowed)
        .unwrap();
    
    window.make_current();
    window.set_key_polling(true);
    window.set_cursor_pos_polling(true);
    window.set_mouse_button_polling(true);
    
    // Load OpenGL functions
    gl::load_with(|symbol| {
        if let Some(addr) = window.get_proc_address(symbol) {
            addr as *const c_void
        } else {
            ptr::null()
        }
    });
    
    unsafe {
        let chunk_renderer = ChunkRenderer::new().unwrap();
        println!("OpenGL initialized! Press ESC to quit.");

        // Enable depth testing:
        gl::Enable(gl::DEPTH_TEST);

        // World
        let mut world = World::new(4);

        // Create player and snap spawn position to terrain surface
        let mut player = Player::new();
        world.update(player.position); // Kick off initial chunk generation
        // Wait for the spawn chunk to be ready before placing the player
        while world.surface_height(0, 0) == 0 {
            world.finalize_all_pending();
        }
        let spawn_y = world.surface_height(0, 0);
        player.position[1] = spawn_y as f32;

        // Camera parameters
        let mut camera = Camera::new(1600, 1200);
        let mut last_mouse_x = 800.0;
        let mut last_mouse_y = 600.0;
        let mut first_mouse = true;

        // Create a key state map BEFORE the main loop
        use std::collections::HashMap;
        let mut keys_pressed: HashMap<Key, bool> = HashMap::new();

        let mut wireframe_mode = false;
        let mut last_frame = Instant::now();

        let crosshair_renderer = crosshair_renderer::Crosshair::new();
        let health_bar = HealthBar::new();
        let menu_renderer = MenuRenderer::new();
        let outline_renderer = BlockOutlineRenderer::new();
        let player_renderer = PlayerRenderer::new();
        let crack_renderer = CrackRenderer::new();
        let item_renderer = ItemRenderer::new();
        let mut item_entities: Vec<ItemEntity> = Vec::new();
        let hotbar_renderer = HotbarRenderer::new();
        let bag_renderer = BagRenderer::new();
        let mut bag_open = false;
        let mut selected_slot: usize = 0;
        let hotbar: [Option<ItemType>; 9] = [None; 9];
        let mut paused = false;
        let mut outline_enabled = true;
        let mut hi_res = true;
        let mut win_w: f32 = 1600.0;
        let mut win_h: f32 = 1200.0;

        // Digging state
        let mut lmb_held = false;
        let mut dig_target: Option<[i32; 3]> = None;
        let mut dig_progress: f32 = 0.0;
        let mut swing_time: f32 = 0.0;

        // Main loop
        while !window.should_close() {
            let now = Instant::now();
            let delta_time = now.duration_since(last_frame).as_secs_f32();
            last_frame = now;

            // Handle events
            glfw.poll_events();
            for (_, event) in glfw::flush_messages(&events) {
                match event {

                    glfw::WindowEvent::CursorPos(x, y) => {
                        let xoffset = x as f32 - last_mouse_x;
                        let yoffset = last_mouse_y - y as f32;
                        last_mouse_x = x as f32;
                        last_mouse_y = y as f32;

                        if !paused && !bag_open {
                            if first_mouse {
                                first_mouse = false;
                            } else {
                                player.process_mouse_movement(xoffset, yoffset);
                            }
                        }
                    }

                    glfw::WindowEvent::MouseButton(glfw::MouseButton::Button1, Action::Press, _) => {
                        if paused {
                            if menu_renderer.is_exit_clicked(last_mouse_x, last_mouse_y, win_w, win_h) {
                                window.set_should_close(true);
                            } else if menu_renderer.is_outline_clicked(last_mouse_x, last_mouse_y, win_w, win_h) {
                                outline_enabled = !outline_enabled;
                            } else if menu_renderer.is_res_clicked(last_mouse_x, last_mouse_y, win_w, win_h) {
                                hi_res = !hi_res;
                                let (new_w, new_h) = if hi_res { (1600, 1200) } else { (800, 600) };
                                window.set_size(new_w, new_h);
                                win_w = new_w as f32;
                                win_h = new_h as f32;
                                camera.on_resize(new_w as u32, new_h as u32);
                                gl::Viewport(0, 0, new_w, new_h);
                            }
                        } else {
                            window.set_cursor_mode(glfw::CursorMode::Disabled);
                            lmb_held = true;
                        }
                    }

                    glfw::WindowEvent::MouseButton(glfw::MouseButton::Button1, Action::Release, _) => {
                        lmb_held = false;
                        // Progress is preserved — it decays over time in the update loop.
                    }

                    glfw::WindowEvent::Key(Key::F12, _, Action::Press, _) => {
                        wireframe_mode = !wireframe_mode;
                        if wireframe_mode {
                            gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE);
                        } else {
                            gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);
                        }
                    }

                    glfw::WindowEvent::Key(key, _, action, modifiers) => {
                        match action {
                            Action::Press => {
                                if key == Key::Escape {
                                    if bag_open {
                                        bag_open = false;
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
                                } else if key == Key::I && !paused {
                                    bag_open = !bag_open;
                                    if bag_open {
                                        window.set_cursor_mode(glfw::CursorMode::Normal);
                                        first_mouse = true;
                                    } else {
                                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                                        first_mouse = true;
                                    }
                                } else if !paused {
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
                            Action::Release => {
                                keys_pressed.insert(key, false);
                            }
                            _ => {}
                        }
                    }

                    _ => {}
                }
            }

            if !paused && !bag_open {
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
                camera.move_to_abs(player.position[0], player.position[1] + 1.6, player.position[2]);

                // Digging
                if !lmb_held {
                    // Decay progress while not digging — full decay in 30 seconds.
                    if let Some(target) = dig_target {
                        let block = world.get_block(target[0], target[1], target[2]);
                        if let Some(hardness) = block.hardness() {
                            dig_progress -= (hardness / 30.0) * delta_time;
                            if dig_progress <= 0.0 {
                                dig_progress = 0.0;
                                dig_target = None;
                            }
                        } else {
                            dig_target = None;
                            dig_progress = 0.0;
                        }
                    }
                }

                if lmb_held {
                    let cam_pos = [camera.position.x, camera.position.y, camera.position.z];
                    let cam_dir = [camera.front.x, camera.front.y, camera.front.z];
                    if let Some(target) = world.raycast(cam_pos, cam_dir, 5.0) {
                        let block = world.get_block(target[0], target[1], target[2]);
                        if Some(target) != dig_target {
                            // Moved to a different block — reset progress
                            dig_target = Some(target);
                            dig_progress = 0.0;
                        }
                        if let Some(hardness) = block.hardness() {
                            // Tool speed multiplier goes here in the future (e.g. * tool.speed())
                            dig_progress += delta_time;
                            if dig_progress >= hardness {
                                let drops = block.drops(target[0], target[1], target[2]);
                                world.set_block(target[0], target[1], target[2], world::BlockType::Air);
                                for item_type in drops {
                                    item_entities.push(ItemEntity::new(
                                        target[0] as f32, target[1] as f32, target[2] as f32, item_type,
                                    ));
                                }
                                dig_target = None;
                                dig_progress = 0.0;
                            }
                        }
                    } else {
                        dig_target = None;
                        dig_progress = 0.0;
                    }
                }

                // Arm swing accumulator — reset on release so next dig starts from rest
                if lmb_held {
                    swing_time += delta_time;
                } else {
                    swing_time = 0.0;
                }

                // Age item entities
                for entity in &mut item_entities {
                    entity.update(delta_time, |x, y, z| world.get_block(x, y, z));
                }

                // Auto-pickup: collect items within 1.5 blocks of the player
                item_entities.retain(|entity| {
                    let dx = entity.position[0] + 0.5 - player.position[0];
                    let dy = entity.position[1] + 0.5 - (player.position[1] + 0.9);
                    let dz = entity.position[2] + 0.5 - player.position[2];
                    if (dx*dx + dy*dy + dz*dz).sqrt() < 1.5 {
                        !player.pick_up(entity.item) // remove if picked up; keep if inventory full
                    } else {
                        true // keep
                    }
                });

                // Update world around player
                world.update(player.position);
                world.tick_water(delta_time);
            }

            // Clear and draw
            gl::ClearColor(0.53, 0.81, 0.92, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

            let view = camera.view_matrix();
            let projection = camera.projection_matrix();

            // Draw terrain
            chunk_renderer.begin_frame(&view, &projection);
            world.draw(&chunk_renderer, &camera);
            chunk_renderer.end_frame();

            // Draw player model
            // SWING_SPEED: future tools can multiply this (axe faster, pick medium, etc.)
            const SWING_SPEED: f32 = 8.0; // rad/s ≈ 1.3 full swings/sec
            // Base amplitude when looking level; pitch shifts the aim: negative pitch (down) → less angle → arm angles down
            const SWING_AMP_BASE: f32 = 1.4; // rad (~80°) — arm reaches mostly forward at pitch=0
            let swing_angle = if lmb_held {
                let pitch_rad = player.pitch.to_radians(); // negative = looking down, positive = looking up
                let target = (SWING_AMP_BASE + pitch_rad).clamp(0.1, std::f32::consts::PI * 0.85);
                (swing_time * SWING_SPEED).sin().abs() * target
            } else {
                0.0
            };
            player_renderer.draw(player.position, player.yaw, &view, &projection, PlayerDrawMode::ArmsOnly, swing_angle);

            // Draw block outline
            if outline_enabled && !paused && !bag_open {
                let cam_pos = [camera.position.x, camera.position.y, camera.position.z];
                let cam_dir = [camera.front.x, camera.front.y, camera.front.z];
                if let Some(block) = world.raycast(cam_pos, cam_dir, 5.0) {
                    outline_renderer.draw(block, &view, &projection);
                }
            }

            // Draw dropped items
            item_renderer.draw(&item_entities, &view, &projection);

            // Draw crack overlay on block being dug
            if lmb_held {
                if let Some(target) = dig_target {
                    let block = world.get_block(target[0], target[1], target[2]);
                    if let Some(hardness) = block.hardness() {
                        let stage = ((dig_progress / hardness * 5.0) as usize).min(4);
                        crack_renderer.draw(target, stage, &view, &projection);
                    }
                }
            }

            // ── HUD (drawn last so nothing 3D renders on top) ──────────────

            // Underwater tint — check the block at eye level
            let eye = camera.position;
            if world.get_block(eye.x.floor() as i32, eye.y.floor() as i32, eye.z.floor() as i32) == world::BlockType::Water {
                hotbar_renderer.draw_fullscreen_tint([0.05, 0.20, 0.60, 0.35], win_w, win_h);
            }

            // Draw crosshair
            crosshair_renderer.draw();

            // Draw health bar
            let health_fraction = player.health as f32 / 100.0;
            health_bar.draw(health_fraction);

            // Draw hotbar
            hotbar_renderer.draw(selected_slot, &hotbar, win_w, win_h);

            // Draw bag
            if bag_open {
                bag_renderer.draw(&player.inventory, win_w, win_h);
            }

            // Draw pause menu
            if paused {
                menu_renderer.draw(outline_enabled, hi_res);
            }

            window.swap_buffers();
        }
        
        // Cleanup
        drop(chunk_renderer);
    }
}