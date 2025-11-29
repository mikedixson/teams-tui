mod api;
mod app;
mod auth;
pub mod config;
pub mod image_display;
mod ui;

use crate::app::{ActivePane, App};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

#[tokio::main]
async fn main() -> Result<()> {
    // Authenticate first (before setting up terminal)
    println!("TeamsTUI");
    println!("================================\n");

    let access_token = match auth::get_access_token().await {
        Ok(token) => {
            println!("✓ Authentication successful!\n");
            token
        }
        Err(e) => {
            eprintln!("✗ Authentication failed: {}", e);
            return Err(e);
        }
    };

    // Fetch current user profile
    println!("Fetching user profile...");
    let current_user = match api::get_me(&access_token).await {
        Ok(user) => {
            println!("✓ Logged in as: {}\n", user.display_name);
            Some(user)
        }
        Err(e) => {
            eprintln!("⚠ Failed to fetch user profile: {}", e);
            None
        }
    };

    // Fetch chats
    println!("Fetching chats...");
    let (chats, _) = match api::get_chats(&access_token).await {
        Ok(result) => {
            println!("✓ Loaded {} chats\n", result.0.len());
            result
        }
        Err(e) => {
            eprintln!("✗ Failed to fetch chats: {}", e);
            return Err(e);
        }
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();
    app.set_chats(chats);
    if let Some(user) = current_user {
        app.set_current_user(user.display_name);
    }

    // Run app
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    // Create a channel for receiving loaded messages
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(usize, Vec<api::Message>)>();

    // Create a channel for receiving chat updates
    let (tx_chats, mut rx_chats) =
        tokio::sync::mpsc::unbounded_channel::<(Vec<api::Chat>, Option<String>)>();

    // Create a channel for receiving loaded images
    let (tx_image, mut rx_image) = tokio::sync::mpsc::unbounded_channel::<(String, Vec<u8>)>();

    // Shared HTTP client for image downloads
    let http_client = std::sync::Arc::new(reqwest::Client::new());

    // Spawn background task to refresh chats
    let tx_chats_clone = tx_chats.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Ok(token) = auth::get_valid_token_silent().await {
                if let Ok(result) = api::get_chats(&token).await {
                    let _ = tx_chats_clone.send(result);
                }
            }
        }
    });

    // Helper function to spawn image download task
    let spawn_image_download = |url: String,
                                tx_img: tokio::sync::mpsc::UnboundedSender<(String, Vec<u8>)>,
                                client: std::sync::Arc<reqwest::Client>| {
        tokio::spawn(async move {
            if let Ok(token) = auth::get_valid_token_silent().await {
                if let Ok(bytes) = image_display::download_image(&client, &url, &token).await {
                    let _ = tx_img.send((url, bytes));
                }
            }
        });
    };

    // Load messages for the first chat if available
    if let Some(chat) = app.get_selected_chat() {
        let chat_id = chat.id.clone();
        let chat_index = app.selected_index;
        let tx_clone = tx.clone();

        app.set_loading_messages(true);
        tokio::spawn(async move {
            if let Ok(token) = auth::get_valid_token_silent().await {
                if let Ok(messages) = api::get_messages(&token, &chat_id).await {
                    let _ = tx_clone.send((chat_index, messages));
                }
            }
        });
    }

    loop {
        // Check for chat updates
        while let Ok((chats, _)) = rx_chats.try_recv() {
            // Preserve selection
            let current_chat_id = app.get_selected_chat().map(|c| c.id.clone());

            app.set_chats(chats);

            if let Some(id) = current_chat_id {
                if let Some(index) = app.chats.iter().position(|c| c.id == id) {
                    app.selected_index = index;

                    // Always refresh messages for the current chat to ensure we get new ones
                    let tx_clone = tx.clone();
                    let chat_id = id.clone();
                    let chat_index = index;

                    tokio::spawn(async move {
                        if let Ok(token) = auth::get_valid_token_silent().await {
                            if let Ok(messages) = api::get_messages(&token, &chat_id).await {
                                let _ = tx_clone.send((chat_index, messages));
                            }
                        }
                    });
                } else {
                    // Chat disappeared or moved, keep index clamped
                    if app.selected_index >= app.chats.len() {
                        app.selected_index = app.chats.len().saturating_sub(1);
                    }
                }
            }
        }

        // Check for loaded messages (non-blocking)
        while let Ok((chat_index, messages)) = rx.try_recv() {
            // Only update if we're still on the same chat
            if chat_index == app.selected_index {
                // Check if messages actually changed to avoid unnecessary snaps/renders
                let should_update = if app.messages.len() != messages.len() {
                    true
                } else {
                    // Check last message ID
                    match (app.messages.last(), messages.last()) {
                        (Some(curr), Some(new)) => curr.id != new.id,
                        (None, None) => false,
                        _ => true,
                    }
                };

                if should_update {
                    app.set_messages(messages);
                    app.snap_to_bottom = true;
                }
            }
        }

        // Check for loaded images
        while let Ok((url, bytes)) = rx_image.try_recv() {
            // Only process if we're still viewing this image
            if let Some(ref viewing) = app.viewing_image {
                if viewing.url == url {
                    // Try to decode and create protocol
                    if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                        if let Some(ref mut picker) = app.image_picker {
                            let protocol = picker.new_resize_protocol(dyn_img);
                            app.set_image_protocol(protocol);
                        }
                    } else {
                        app.loading_image = false;
                    }
                }
            }
        }

        terminal.draw(|f| ui::draw(f, app))?;

        // Use poll with timeout to allow checking for messages
        if event::poll(std::time::Duration::from_millis(100))? {
            let previous_index = app.selected_index;

            match event::read()? {
                Event::Key(key) => {
                    // Handle image viewing mode first
                    if app.is_viewing_image() {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => {
                                app.stop_viewing_image();
                            }
                            KeyCode::Left | KeyCode::Char('h') => {
                                app.previous_image();
                                // Load the new image
                                if let Some(img) = app.get_current_viewable_image().cloned() {
                                    let url = img.url.clone();
                                    app.start_viewing_image(img);
                                    spawn_image_download(
                                        url,
                                        tx_image.clone(),
                                        http_client.clone(),
                                    );
                                }
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                app.next_image();
                                // Load the new image
                                if let Some(img) = app.get_current_viewable_image().cloned() {
                                    let url = img.url.clone();
                                    app.start_viewing_image(img);
                                    spawn_image_download(
                                        url,
                                        tx_image.clone(),
                                        http_client.clone(),
                                    );
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Normal key handling
                    match key.code {
                        KeyCode::Char('q') if !app.input_mode => return Ok(()),
                        KeyCode::Down | KeyCode::Char('j') if !app.input_mode => app.next_chat(),
                        KeyCode::Up | KeyCode::Char('k') if !app.input_mode => app.previous_chat(),
                        KeyCode::Char('v') if !app.input_mode => {
                            // View image - open image viewer if images are available
                            if let Some(img) = app.get_current_viewable_image().cloned() {
                                let url = img.url.clone();
                                app.start_viewing_image(img);
                                spawn_image_download(url, tx_image.clone(), http_client.clone());
                            }
                        }
                        KeyCode::Char('i') if !app.input_mode => {
                            app.input_mode = true;
                            app.input_buffer.clear();
                        }
                        KeyCode::Esc if app.input_mode => {
                            app.input_mode = false;
                            app.input_buffer.clear();
                        }
                        KeyCode::Enter if app.input_mode => {
                            if !app.input_buffer.is_empty() {
                                let message = app.input_buffer.clone();
                                app.input_buffer.clear();
                                app.input_mode = false;

                                // Send message logic
                                if let Some(chat) = app.get_selected_chat() {
                                    let chat_id = chat.id.clone();
                                    let chat_index = app.selected_index;
                                    let tx = tx.clone();
                                    let tx_chats = tx_chats.clone();

                                    app.snap_to_bottom = true;
                                    tokio::spawn(async move {
                                        if let Ok(token) = auth::get_valid_token_silent().await {
                                            if api::send_message(&token, &chat_id, &message)
                                                .await
                                                .is_ok()
                                            {
                                                // Reload messages
                                                if let Ok(messages) =
                                                    api::get_messages(&token, &chat_id).await
                                                {
                                                    let _ = tx.send((chat_index, messages));
                                                }
                                                // Refresh chat list to update last message preview
                                                if let Ok(chats) = api::get_chats(&token).await {
                                                    let _ = tx_chats.send(chats);
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        KeyCode::Backspace if app.input_mode => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Char(c) if app.input_mode => {
                            app.input_buffer.push(c);
                        }
                        KeyCode::PageUp => {
                            app.snap_to_bottom = false;
                            app.scroll_offset = app.scroll_offset.saturating_sub(10);
                        }
                        KeyCode::PageDown => {
                            app.scroll_offset = app.scroll_offset.saturating_add(10);
                            if app.scroll_offset >= app.max_scroll {
                                app.snap_to_bottom = true;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse_event) => {
                    let x = mouse_event.column;
                    let y = mouse_event.row;

                    // Determine which pane was clicked/scrolled
                    let in_chat_list = x >= app.chat_list_area.x
                        && x < app.chat_list_area.x + app.chat_list_area.width
                        && y >= app.chat_list_area.y
                        && y < app.chat_list_area.y + app.chat_list_area.height;

                    let in_messages = x >= app.messages_area.x
                        && x < app.messages_area.x + app.messages_area.width
                        && y >= app.messages_area.y
                        && y < app.messages_area.y + app.messages_area.height;

                    match mouse_event.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            if in_chat_list {
                                app.active_pane = ActivePane::ChatList;

                                // Calculate which chat was clicked (accounting for border)
                                let inner_y = y.saturating_sub(app.chat_list_area.y + 1);
                                let clicked_index = inner_y as usize;

                                if clicked_index < app.chats.len() {
                                    app.selected_index = clicked_index;
                                }
                            } else if in_messages {
                                app.active_pane = ActivePane::Messages;
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if in_chat_list {
                                app.active_pane = ActivePane::ChatList;
                                // Scroll chat list up
                                if app.selected_index > 0 {
                                    app.selected_index -= 1;
                                }
                            } else if in_messages {
                                app.active_pane = ActivePane::Messages;
                                // Scroll messages up
                                app.snap_to_bottom = false;
                                app.scroll_offset = app.scroll_offset.saturating_sub(3);
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            if in_chat_list {
                                app.active_pane = ActivePane::ChatList;
                                // Scroll chat list down
                                if !app.chats.is_empty() && app.selected_index < app.chats.len() - 1
                                {
                                    app.selected_index += 1;
                                }
                            } else if in_messages {
                                app.active_pane = ActivePane::Messages;
                                // Scroll messages down
                                app.scroll_offset = app.scroll_offset.saturating_add(3);
                                if app.scroll_offset >= app.max_scroll {
                                    app.snap_to_bottom = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }

            // If selection changed, spawn a background task to load messages
            if previous_index != app.selected_index {
                if let Some(chat) = app.get_selected_chat() {
                    let chat_id = chat.id.clone();
                    let chat_index = app.selected_index;
                    let tx_clone = tx.clone();

                    app.set_loading_messages(true);
                    app.set_messages(Vec::new()); // Clear old messages immediately
                    app.snap_to_bottom = true; // Snap to bottom for new chat

                    tokio::spawn(async move {
                        if let Ok(token) = auth::get_valid_token_silent().await {
                            if let Ok(messages) = api::get_messages(&token, &chat_id).await {
                                let _ = tx_clone.send((chat_index, messages));
                            }
                        }
                    });
                }
            }
        }
    }
}
