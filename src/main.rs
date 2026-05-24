use std::sync::Arc;

use rfd;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowAttributes},
};

mod editor;
mod renderer;

use editor::EditorBuffer;
use renderer::{Renderer, LINE_HEIGHT};

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    editor: EditorBuffer,
    modifiers: ModifiersState,
    file_arg: Option<String>,
}

impl App {
    fn new(file_arg: Option<String>) -> Self {
        Self {
            window: None,
            renderer: None,
            editor: EditorBuffer::new(),
            modifiers: ModifiersState::default(),
            file_arg,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("Editor")
            .with_inner_size(winit::dpi::LogicalSize::new(1000u32, 700u32));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let renderer = pollster::block_on(Renderer::new(window.clone()));
        if let Some(path) = self.file_arg.take() {
            self.editor.load_file(&path);
        }
        self.window = Some(window.clone());
        self.renderer = Some(renderer);
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => {
                let visible_h = self
                    .window
                    .as_ref()
                    .map(|w| w.inner_size().height as f32)
                    .unwrap_or(700.0);
                self.editor.scroll_to_cursor(LINE_HEIGHT, visible_h);
                if let Some(r) = &mut self.renderer {
                    r.render(&self.editor);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        text,
                        ..
                    },
                ..
            } => {
                let ctrl = self.modifiers.control_key();
                match &logical_key {
                    Key::Named(NamedKey::ArrowLeft) => self.editor.move_left(),
                    Key::Named(NamedKey::ArrowRight) => self.editor.move_right(),
                    Key::Named(NamedKey::ArrowUp) => self.editor.move_up(),
                    Key::Named(NamedKey::ArrowDown) => self.editor.move_down(),
                    Key::Named(NamedKey::Home) => self.editor.move_home(),
                    Key::Named(NamedKey::End) => self.editor.move_end(),
                    Key::Named(NamedKey::Backspace) => self.editor.backspace(),
                    Key::Named(NamedKey::Delete) => self.editor.delete(),
                    Key::Named(NamedKey::Enter) => self.editor.insert_newline(),
                    Key::Named(NamedKey::Tab) => self.editor.insert_str("\t"),
                    Key::Character(c) if ctrl => match c.as_str() {
                        "s" | "S" if self.modifiers.shift_key() => {
                            if let Some(path) = rfd::FileDialog::new().save_file() {
                                self.editor.save_to(path);
                            }
                        }
                        "s" | "S" => self.editor.save(),
                        "o" | "O" => {
                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                self.editor.load_file(path.to_str().unwrap_or(""));
                            }
                        }
                        _ => {}
                    },
                    _ => {
                        if !ctrl {
                            if let Some(t) = text {
                                self.editor.insert_str(&t);
                            }
                        }
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            _ => {}
        }
    }
}

fn main() {
    let file_arg = std::env::args().nth(1);
    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut App::new(file_arg)).unwrap();
}
