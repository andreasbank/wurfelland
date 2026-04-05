use glfw::{Action, Context, Key};
use std::os::raw::c_void;
use std::ptr;
use std::time::Instant;

mod game;
use game::Player;

mod camera;
use camera::Camera;

mod world;
use world::World;

mod renderer;
use renderer::ChunkRenderer;
use renderer::crosshair_renderer;
use renderer::HealthBar;
use renderer::MenuRenderer;
use renderer::BlockOutlineRenderer;
use renderer::PlayerRenderer;
use renderer::player_renderer::PlayerDrawMode;


fn main() {
    // Initialize GLFW
    let mut glfw = glfw::init_no_callbacks().unwrap();
    
    // Create window
    let (mut window, events) = glfw
        .create_window(1600, 1200, "OpenGL Triangle", glfw::WindowMode::Windowed)
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
        let mut player = Player::new("Player1");
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

        let mut crosshair_renderer = crosshair_renderer::Crosshair::new();
        let health_bar = HealthBar::new();
        let menu_renderer = MenuRenderer::new();
        let outline_renderer = BlockOutlineRenderer::new();
        let player_renderer = PlayerRenderer::new();
        let mut paused = false;
        let mut outline_enabled = true;
        let mut hi_res = true;
        let mut win_w: f32 = 1600.0;
        let mut win_h: f32 = 1200.0;

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

                        if !paused {
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
                        }
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
                                    paused = !paused;
                                    if paused {
                                        window.set_cursor_mode(glfw::CursorMode::Normal);
                                        first_mouse = true;
                                    } else {
                                        window.set_cursor_mode(glfw::CursorMode::Disabled);
                                    }
                                } else if !paused {
                                    if key == Key::Space {
                                        player.jump();
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

            if !paused {
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

                // Update world around player
                world.update(player.position);
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

            // Draw crosshair
            crosshair_renderer.draw();

            // Draw health bar
            let health_fraction = player.health as f32 / 100.0;
            health_bar.draw(health_fraction);

            // Draw player model
            player_renderer.draw(player.position, player.yaw, &view, &projection, PlayerDrawMode::ArmsOnly);

            // Draw block outline
            if outline_enabled && !paused {
                let cam_pos = [camera.position.x, camera.position.y, camera.position.z];
                let cam_dir = [camera.front.x, camera.front.y, camera.front.z];
                if let Some(block) = world.raycast(cam_pos, cam_dir, 5.0) {
                    outline_renderer.draw(block, &view, &projection);
                }
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