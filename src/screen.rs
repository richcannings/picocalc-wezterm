use core::fmt;
use core::ops::{Deref, DerefMut};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::lazy_lock::LazyLock;
use embassy_sync::mutex::Mutex as AsyncMutex;
use embassy_time::{Duration, Ticker};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::{Rgb565, Rgb888};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::*;
use embedded_graphics::text::Text;
use embassy_embedded_hal::shared_bus::blocking::spi::SpiDeviceWithConfig;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI1;
use mipidsi::interface::SpiInterface;
use mipidsi::models::ILI9488Rgb565;

extern crate alloc;
use alloc::vec::Vec;
use alloc::vec;

pub const SCREEN_HEIGHT: u16 = 320;
pub const SCREEN_WIDTH: u16 = 320;

// Define PicoCalcDisplay here so it can be used in main.rs and here
pub type PicoCalcDisplay<'a> = mipidsi::Display<
    SpiInterface<
        'a,
        SpiDeviceWithConfig<
            'a,
            NoopRawMutex,
            embassy_rp::spi::Spi<'a, SPI1, embassy_rp::spi::Blocking>,
            Output<'a>,
        >,
        Output<'a>,
    >,
    ILI9488Rgb565,
    Output<'a>,
>;

static FONTS: &[&MonoFont] = &[
    &profont::PROFONT_7_POINT,
    &profont::PROFONT_9_POINT,
    &profont::PROFONT_10_POINT,
    &profont::PROFONT_12_POINT,
    &profont::PROFONT_14_POINT,
    &profont::PROFONT_18_POINT,
    &profont::PROFONT_24_POINT,
];

pub static SCREEN: LazyLock<AsyncMutex<CriticalSectionRawMutex, Screen>> =
    LazyLock::new(|| AsyncMutex::new(Screen::new()));

pub struct Screen {
    model: ScreenModel,
    parser: vte::Parser,
}

impl Deref for Screen {
    type Target = ScreenModel;
    fn deref(&self) -> &ScreenModel {
        &self.model
    }
}

impl DerefMut for Screen {
    fn deref_mut(&mut self) -> &mut ScreenModel {
        &mut self.model
    }
}

impl Screen {
    pub fn new() -> Self {
        Self {
            model: ScreenModel::default(),
            parser: vte::Parser::new(),
        }
    }

    pub fn parse_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.parser.advance(&mut self.model, *byte);
        }
    }

    pub fn print(&mut self, text: &str) {
        self.parse_bytes(text.as_bytes())
    }
    
    pub fn clear(&mut self) {
        self.model.clear();
    }
}

impl fmt::Write for Screen {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.print(s);
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    DefaultFg,
    DefaultBg,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

impl Color {
    fn to_rgb565(self, is_bg: bool) -> Rgb565 {
        match self {
            Color::Black => Rgb565::BLACK,
            Color::Red => Rgb565::RED,
            Color::Green => Rgb565::GREEN,
            Color::Yellow => Rgb565::YELLOW,
            Color::Blue => Rgb565::BLUE,
            Color::Magenta => Rgb565::MAGENTA,
            Color::Cyan => Rgb565::CYAN,
            Color::White => Rgb565::WHITE,
            Color::BrightBlack => Rgb565::new(10, 20, 10), // Approx
            Color::BrightRed => Rgb565::new(31, 20, 20),
            Color::BrightGreen => Rgb565::new(20, 63, 20),
            Color::BrightYellow => Rgb565::new(31, 63, 20),
            Color::BrightBlue => Rgb565::new(20, 20, 31),
            Color::BrightMagenta => Rgb565::new(31, 20, 31),
            Color::BrightCyan => Rgb565::new(20, 63, 31),
            Color::BrightWhite => Rgb565::WHITE,
            Color::DefaultFg => Rgb565::CSS_LIGHT_GRAY,
            Color::DefaultBg => Rgb565::BLACK,
            Color::Rgb(r, g, b) => Rgb888::new(r, g, b).into(),
            Color::Indexed(i) => {
                // Simple mapping for first 16 colors, else default
                if i < 8 {
                    // map to standard colors
                    match i {
                        0 => Rgb565::BLACK,
                        1 => Rgb565::RED,
                        2 => Rgb565::GREEN,
                        3 => Rgb565::YELLOW,
                        4 => Rgb565::BLUE,
                        5 => Rgb565::MAGENTA,
                        6 => Rgb565::CYAN,
                        7 => Rgb565::CSS_LIGHT_GRAY,
                        _ => Rgb565::WHITE,
                    }
                } else if i < 16 {
                    // brights
                     match i {
                        8 => Rgb565::new(10, 20, 10),
                        9 => Rgb565::new(31, 20, 20),
                        10 => Rgb565::new(20, 63, 20),
                        11 => Rgb565::new(31, 63, 20),
                        12 => Rgb565::new(20, 20, 31),
                        13 => Rgb565::new(31, 20, 31),
                        14 => Rgb565::new(20, 63, 31),
                        15 => Rgb565::WHITE,
                        _ => Rgb565::WHITE,
                    }
                } else {
                    if is_bg { Rgb565::BLACK } else { Rgb565::WHITE }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Attrs {
    fg: Color,
    bg: Color,
    bold: bool,
    underline: bool,
    reverse: bool,
}

impl Default for Attrs {
    fn default() -> Self {
        Self {
            fg: Color::DefaultFg,
            bg: Color::DefaultBg,
            bold: false,
            underline: false,
            reverse: false,
        }
    }
}

#[derive(Clone)]
struct Line {
    chars: Vec<char>,
    attrs: Vec<Attrs>,
    dirty: bool,
}

impl Line {
    fn new(width: usize) -> Self {
        Self {
            chars: vec![' '; width],
            attrs: vec![Attrs::default(); width],
            dirty: true,
        }
    }
    
    fn clear(&mut self) {
        for c in self.chars.iter_mut() { *c = ' '; }
        for a in self.attrs.iter_mut() { *a = Attrs::default(); }
        self.dirty = true;
    }
}

pub struct ScreenModel {
    lines: Vec<Line>,
    cursor_x: usize,
    cursor_y: usize,
    current_attrs: Attrs,
    font: &'static MonoFont<'static>,
    rows: usize,
    cols: usize,
    full_repaint: bool,
}

impl Default for ScreenModel {
    fn default() -> Self {
        let font = FONTS[2];
        let cols = ((SCREEN_WIDTH as u32) / (font.character_size.width + font.character_spacing)) as usize;
        let rows = ((SCREEN_HEIGHT as u32) / font.character_size.height) as usize;
        
        // Initialize lines
        let mut lines = Vec::with_capacity(rows);
        for _ in 0..rows {
            lines.push(Line::new(cols));
        }

        Self {
            lines,
            cursor_x: 0,
            cursor_y: 0,
            current_attrs: Attrs::default(),
            font,
            rows,
            cols,
            full_repaint: true,
        }
    }
}

impl ScreenModel {
    pub fn width(&self) -> u16 {
        self.cols as u16
    }

    pub fn height(&self) -> u16 {
        self.rows as u16
    }
    
    pub fn clear(&mut self) {
        for line in self.lines.iter_mut() {
            line.clear();
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.full_repaint = true;
    }

    pub fn increase_font(&mut self) {
        // TODO: implement font resizing
    }

    pub fn decrease_font(&mut self) {
        // TODO: implement font resizing
    }

    fn scroll_up(&mut self) {
        // Remove first line, add new line at end
        if !self.lines.is_empty() {
            self.lines.remove(0);
            self.lines.push(Line::new(self.cols));
            self.full_repaint = true; // Simple repaint for scroll
        }
    }

    pub fn update_display(&mut self, display: &mut PicoCalcDisplay) {
        if self.full_repaint {
            display.clear(Rgb565::BLACK).unwrap();
        }

        let font = self.font;
        let cell_width = font.character_size.width + font.character_spacing;
        let cell_height = font.character_size.height;

        for (y, line) in self.lines.iter_mut().enumerate() {
            if !line.dirty && !self.full_repaint {
                continue;
            }
            
            let row_y = y as u32 * cell_height as u32;
            if row_y >= SCREEN_HEIGHT as u32 { break; }

            for (x, (char, attr)) in line.chars.iter().zip(line.attrs.iter()).enumerate() {
                let col_x = x as u32 * cell_width;
                if col_x >= SCREEN_WIDTH as u32 { break; }

                let mut fg = attr.fg.to_rgb565(false);
                let mut bg = attr.bg.to_rgb565(true);
                
                if attr.reverse {
                    core::mem::swap(&mut fg, &mut bg);
                }
                
                if attr.bold {
                    // Brighten fg?
                    if fg == Rgb565::CSS_LIGHT_GRAY { fg = Rgb565::WHITE; }
                }

                // Draw background
                display.fill_solid(
                    &Rectangle::new(
                        Point::new(col_x as i32, row_y as i32),
                        Size::new(cell_width, cell_height as u32),
                    ),
                    bg,
                ).unwrap();

                // Draw text
                if *char != ' ' {
                     let style = MonoTextStyleBuilder::new()
                        .font(font)
                        .text_color(fg)
                        .background_color(bg)
                        .build();
                    
                    // We need to handle char string
                    let mut buf = [0u8; 4];
                    let s = char.encode_utf8(&mut buf);

                    Text::new(
                        s,
                        Point::new(col_x as i32, (row_y as i32 + font.baseline as i32)),
                        style,
                    )
                    .draw(display)
                    .unwrap();
                }
                
                if attr.underline {
                     display.fill_solid(
                        &Rectangle::new(
                            Point::new(col_x as i32, (row_y + cell_height as u32 - 1) as i32),
                            Size::new(cell_width, 1),
                        ),
                        fg,
                    ).unwrap();
                }
            }
            line.dirty = false;
        }
        self.full_repaint = false;

        // Draw cursor
        let cx = self.cursor_x as u32 * cell_width;
        let cy = self.cursor_y as u32 * cell_height as u32;
        if cx < SCREEN_WIDTH as u32 && cy < SCREEN_HEIGHT as u32 {
             display.fill_solid(
                &Rectangle::new(
                    Point::new(cx as i32, cy as i32),
                    Size::new(cell_width, cell_height as u32),
                ),
                Rgb565::WHITE, 
            ).ok();
        }
    }
}

impl vte::Perform for ScreenModel {
    fn print(&mut self, c: char) {
        if self.cursor_y >= self.rows {
            self.scroll_up();
            self.cursor_y = self.rows - 1;
        }
        if self.cursor_x >= self.cols {
            self.cursor_x = 0;
            self.cursor_y += 1;
            if self.cursor_y >= self.rows {
                self.scroll_up();
                self.cursor_y = self.rows - 1;
            }
        }
        
        let line = &mut self.lines[self.cursor_y];
        if self.cursor_x < line.chars.len() {
            line.chars[self.cursor_x] = c;
            line.attrs[self.cursor_x] = self.current_attrs;
            line.dirty = true;
            self.cursor_x += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => { // LF
                self.cursor_y += 1;
                if self.cursor_y >= self.rows {
                    self.scroll_up();
                    self.cursor_y = self.rows - 1;
                }
            }
            b'\r' => { // CR
                self.cursor_x = 0;
            }
            b'\x08' => { // BS
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore || !intermediates.is_empty() { return; }

        match action {
            'A' => { // Cursor Up
                let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cursor_y = self.cursor_y.saturating_sub(n);
            }
            'B' => { // Cursor Down
                let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cursor_y = (self.cursor_y + n).min(self.rows - 1);
            }
            'C' => { // Cursor Forward
                let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cursor_x = (self.cursor_x + n).min(self.cols - 1);
            }
            'D' => { // Cursor Backward
                let n = params.iter().next().map(|p| p[0]).unwrap_or(1) as usize;
                self.cursor_x = self.cursor_x.saturating_sub(n);
            }
            'H' | 'f' => { // Cursor Position
                let mut iter = params.iter();
                let row = iter.next().map(|p| p[0]).unwrap_or(1).max(1) as usize - 1;
                let col = iter.next().map(|p| p[0]).unwrap_or(1).max(1) as usize - 1;
                self.cursor_y = row.min(self.rows - 1);
                self.cursor_x = col.min(self.cols - 1);
            }
            'J' => { // Erase in Display
                let n = params.iter().next().map(|p| p[0]).unwrap_or(0);
                match n {
                    0 => { // Cursor to end
                        // Clear current line from cursor
                        let line = &mut self.lines[self.cursor_y];
                        for i in self.cursor_x..self.cols {
                            line.chars[i] = ' ';
                            line.attrs[i] = self.current_attrs;
                        }
                        line.dirty = true;
                        // Clear lines below
                        for i in (self.cursor_y + 1)..self.rows {
                            self.lines[i].clear();
                        }
                    }
                    1 => { // Beginning to cursor
                        // Clear lines above
                        for i in 0..self.cursor_y {
                            self.lines[i].clear();
                        }
                        // Clear current line up to cursor
                        let line = &mut self.lines[self.cursor_y];
                        for i in 0..=self.cursor_x {
                            line.chars[i] = ' ';
                            line.attrs[i] = self.current_attrs;
                        }
                        line.dirty = true;
                    }
                    2 => { // Entire screen
                        self.clear();
                    }
                    _ => {}
                }
            }
            'K' => { // Erase in Line
                let n = params.iter().next().map(|p| p[0]).unwrap_or(0);
                let line = &mut self.lines[self.cursor_y];
                match n {
                    0 => { // Cursor to end
                        for i in self.cursor_x..self.cols {
                            line.chars[i] = ' ';
                            line.attrs[i] = self.current_attrs;
                        }
                    }
                    1 => { // Beginning to cursor
                        for i in 0..=self.cursor_x {
                            line.chars[i] = ' ';
                            line.attrs[i] = self.current_attrs;
                        }
                    }
                    2 => { // Entire line
                        for i in 0..self.cols {
                            line.chars[i] = ' ';
                            line.attrs[i] = self.current_attrs;
                        }
                    }
                    _ => {}
                }
                line.dirty = true;
            }
            'm' => { // SGR
                for param in params.iter() {
                    let p = param[0];
                    match p {
                        0 => self.current_attrs = Attrs::default(),
                        1 => self.current_attrs.bold = true,
                        4 => self.current_attrs.underline = true,
                        7 => self.current_attrs.reverse = true,
                        22 => self.current_attrs.bold = false,
                        24 => self.current_attrs.underline = false,
                        27 => self.current_attrs.reverse = false,
                        30..=37 => self.current_attrs.fg = Color::Indexed((p - 30) as u8),
                        39 => self.current_attrs.fg = Color::DefaultFg,
                        40..=47 => self.current_attrs.bg = Color::Indexed((p - 40) as u8),
                        49 => self.current_attrs.bg = Color::DefaultBg,
                        90..=97 => self.current_attrs.fg = Color::Indexed((p - 90 + 8) as u8),
                        100..=107 => self.current_attrs.bg = Color::Indexed((p - 100 + 8) as u8),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    
    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

#[embassy_executor::task]
pub async fn screen_painter(mut display: PicoCalcDisplay<'static>) {
    display.clear(Rgb565::BLACK).unwrap();
    if let Err(err) = display.set_vertical_scroll_region(0, 0) {
        // log::error!("failed to set_vertical_scroll_region: {err:?}");
    }

    let mut ticker = Ticker::every(Duration::from_millis(200));
    loop {
        SCREEN.get().lock().await.update_display(&mut display);
        ticker.next().await;
    }
}

pub async fn cls_command(_args: &[&str]) {
    SCREEN.get().lock().await.clear();
}
