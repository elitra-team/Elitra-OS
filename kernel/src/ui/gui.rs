use crate::spinlock::SpinLock;

// ─── Window types ─────────────────────────────────────────────

pub const MAX_WINDOWS: usize = 32;
pub const MAX_TITLE_LEN: usize = 32;

#[derive(Copy, Clone, PartialEq)]
pub enum WindowState {
    Hidden = 0,
    Visible = 1,
    Minimized = 2,
    Focused = 3,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w as i32 &&
        py >= self.y && py < self.y + self.h as i32
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w as i32 &&
        self.x + self.w as i32 > other.x &&
        self.y < other.y + other.h as i32 &&
        self.y + self.h as i32 > other.y
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Window {
    pub id: u32,
    pub state: WindowState,
    pub bounds: Rect,
    pub z_order: u32,
    pub title: [u8; MAX_TITLE_LEN],
    pub needs_redraw: bool,
    pub owner_pid: u32,
    // Framebuffer: each window has its own backing buffer
    pub fb: *mut u8,
    pub fb_pitch: u32,
    pub fb_w: u32,
    pub fb_h: u32,
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

impl Window {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            state: WindowState::Hidden,
            bounds: Rect::new(0, 0, 0, 0),
            z_order: 0,
            title: [0u8; MAX_TITLE_LEN],
            needs_redraw: false,
            owner_pid: 0,
            fb: core::ptr::null_mut(),
            fb_pitch: 0,
            fb_w: 0,
            fb_h: 0,
        }
    }

    pub fn title_str(&self) -> &str {
        let mut len = 0;
        while len < MAX_TITLE_LEN && self.title[len] != 0 {
            len += 1;
        }
        unsafe { core::str::from_utf8_unchecked(&self.title[..len]) }
    }
}

// ─── Event types ──────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq)]
pub enum EventType {
    None = 0,
    MouseMove = 1,
    MouseDown = 2,
    MouseUp = 3,
    KeyDown = 4,
    KeyUp = 5,
    WindowClose = 6,
    WindowResize = 7,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Event {
    pub kind: EventType,
    pub data1: u32,
    pub data2: u32,
    pub data3: u32,
    pub window_id: u32,
}

impl Event {
    pub const fn none() -> Self {
        Self { kind: EventType::None, data1: 0, data2: 0, data3: 0, window_id: 0 }
    }
}

// ─── Compositor state ─────────────────────────────────────────

struct CompositorState {
    windows: [Window; MAX_WINDOWS],
    window_count: u32,
    z_counter: u32,
    focused_window: u32,
    event_queue: [Event; 256],
    event_head: u32,
    event_tail: u32,
    // Screen dimensions
    screen_w: u32,
    screen_h: u32,
    // Double-buffer back
    backbuffer: *mut u8,
    back_pitch: u32,
    back_bpp: u8,
    dirty: bool,
}

unsafe impl Send for CompositorState {}

static COMPOSITOR: SpinLock<CompositorState> = SpinLock::new(CompositorState {
    windows: [Window::empty(); MAX_WINDOWS],
    window_count: 0,
    z_counter: 0,
    focused_window: 0,
    event_queue: [Event::none(); 256],
    event_head: 0,
    event_tail: 0,
    screen_w: 0,
    screen_h: 0,
    backbuffer: core::ptr::null_mut(),
    back_pitch: 0,
    back_bpp: 0,
    dirty: false,
});

// ─── Colors ───────────────────────────────────────────────────

pub const COLOR_DESKTOP: u32 = 0x1A1A2E;
pub const COLOR_TASKBAR: u32 = 0x16213E;
pub const COLOR_TITLEBAR: u32 = 0x0F3460;
pub const COLOR_TITLEBAR_ACTIVE: u32 = 0x533483;
pub const COLOR_TITLEBAR_TEXT: u32 = 0xE0E0E0;
pub const COLOR_WINDOW_BG: u32 = 0x1E1E2E;
pub const COLOR_WINDOW_BORDER: u32 = 0x333355;
pub const COLOR_CLOSE_BTN: u32 = 0xE74C3C;
pub const COLOR_WHITE: u32 = 0xFFFFFF;
pub const COLOR_MOUSE: u32 = 0xE0E0E0;

const TASKBAR_HEIGHT: u32 = 32;
const TITLEBAR_HEIGHT: u32 = 24;
const BORDER_SIZE: u32 = 2;
const MIN_WIDTH: u32 = 120;
const MIN_HEIGHT: u32 = 60;

// ─── Internal helpers ─────────────────────────────────────────

unsafe fn fb_put_pixel(x: u32, y: u32, color: u32, fb: *mut u8, pitch: u32, bpp: u8) {
    if fb.is_null() || x >= pitch / (bpp as u32 / 8) { return; }
    if bpp == 32 {
        let idx = (y as usize) * (pitch as usize / 4) + (x as usize);
        core::ptr::write_volatile(fb.add(idx * 4) as *mut u32, color);
    } else if bpp == 24 {
        let off = (y as usize) * (pitch as usize) + (x as usize) * 3;
        core::ptr::write_volatile(fb.add(off), ((color >> 16) & 0xFF) as u8);
        core::ptr::write_volatile(fb.add(off + 1), ((color >> 8) & 0xFF) as u8);
        core::ptr::write_volatile(fb.add(off + 2), (color & 0xFF) as u8);
    }
}

unsafe fn fb_fill_rect(x: i32, y: i32, w: u32, h: u32, color: u32, fb: *mut u8, screen_w: u32, pitch: u32, bpp: u8) {
    let x0 = if x < 0 { 0 } else { x as u32 };
    let y0 = if y < 0 { 0 } else { y as u32 };
    let x1 = core::cmp::min(x0 + w, screen_w);
    let y1 = core::cmp::min(y0 + h, pitch / (bpp as u32 / 8));

    if bpp == 32 {
        let pitch_dwords = pitch as usize / 4;
        for py in y0..y1 {
            let row_start = py as usize * pitch_dwords + x0 as usize;
            for px in x0..x1 {
                core::ptr::write_volatile(fb.add((row_start + px as usize) * 4) as *mut u32, color);
            }
        }
    }
}

unsafe fn fb_draw_char(x: u32, y: u32, ch: u8, fg: u32, fb: *mut u8, _screen_w: u32, pitch: u32, bpp: u8) {
    let font = crate::framebuffer::FONT8X16;
    let idx = if (ch as usize) < font.len() { ch as usize } else { 0 };
    let glyph = &font[idx];
    for row in 0..16u32 {
        let bits = glyph[row as usize];
        for col in 0..8u32 {
            if bits & (0x80 >> col) != 0 {
                fb_put_pixel(x + col, y + row, fg, fb, pitch, bpp);
            }
        }
    }
}

unsafe fn fb_draw_string(x: u32, y: u32, s: &[u8], fg: u32, fb: *mut u8, screen_w: u32, pitch: u32, bpp: u8) {
    let mut cx = x;
    for &ch in s {
        if ch == 0 { break; }
        fb_draw_char(cx, y, ch, fg, fb, screen_w, pitch, bpp);
        cx += 8;
    }
}

static MOUSE_POS: SpinLock<(i32, i32, bool, u32, i32, i32)> = SpinLock::new((400, 300, false, 0, 0, 0));

// ─── Public API ───────────────────────────────────────────────

/// Initialize compositor with screen dimensions
pub unsafe fn compositor_init(screen_w: u32, screen_h: u32) {
    let mut c = COMPOSITOR.lock();
    c.screen_w = screen_w;
    c.screen_h = screen_h;
    c.focused_window = 0;
    c.z_counter = 0;
    // Allocate backbuffer (4 bytes per pixel)
    let _size = (screen_w * screen_h * 4) as usize;
    let frame = crate::pmm::krust_pmm_alloc_frame();
    if frame != 0 {
        // Use mapped memory at the framebuffer address + offset
        c.backbuffer = core::ptr::null_mut(); // use direct FB for now
    }
    c.dirty = true;
}

/// Create a new window
pub fn window_create(title: &[u8], x: i32, y: i32, w: u32, h: u32, owner_pid: u32) -> i32 {
    let mut c = COMPOSITOR.lock();
    if c.window_count >= MAX_WINDOWS as u32 { return -1; }

    let id = c.window_count + 1;
    let idx = c.window_count as usize;
    c.z_counter += 1;

    let z = c.z_counter;
    let win = &mut c.windows[idx];
    win.id = id;
    win.state = WindowState::Visible;
    win.bounds = Rect::new(x, y, w, h);
    win.z_order = z;
    win.owner_pid = owner_pid;
    win.needs_redraw = true;
    win.fb_pitch = w * 4;
    win.fb_w = w;
    win.fb_h = h;

    // Copy title
    let tlen = if title.len() > MAX_TITLE_LEN { MAX_TITLE_LEN } else { title.len() };
    for i in 0..tlen { win.title[i] = title[i]; }
    for i in tlen..MAX_TITLE_LEN { win.title[i] = 0; }

    // Allocate window framebuffer
    let _fb_size = (w * h * 4) as usize;
    let frame = unsafe { crate::pmm::krust_pmm_alloc_frame() };
    if frame != 0 {
        win.fb = (frame * 4096) as *mut u8;
        // Clear to window bg
        unsafe {
            for i in 0..(w * h) as usize {
                core::ptr::write_volatile(win.fb.add(i * 4) as *mut u32, COLOR_WINDOW_BG);
            }
        }
    }

    c.window_count += 1;
    c.focused_window = id;
    c.dirty = true;
    id as i32
}

/// Destroy a window
pub fn window_destroy(win_id: u32) {
    let mut c = COMPOSITOR.lock();
    let mut found = false;
    for i in 0..c.window_count as usize {
        if c.windows[i].id == win_id {
            // Free framebuffer
            if !c.windows[i].fb.is_null() {
                unsafe {
                    crate::pmm::krust_pmm_free_frame(c.windows[i].fb as usize / 4096);
                }
            }
            // Shift remaining windows
            for j in i..(c.window_count as usize - 1) {
                c.windows[j] = c.windows[j + 1];
            }
            c.window_count -= 1;
            let empty_idx = c.window_count as usize;
            c.windows[empty_idx] = Window::empty();
            found = true;
            break;
        }
    }
    if found {
        if c.focused_window == win_id {
            c.focused_window = if c.window_count > 0 { c.windows[c.window_count as usize - 1].id } else { 0 };
        }
        c.dirty = true;
    }
}

/// Move a window
pub fn window_move(win_id: u32, x: i32, y: i32) {
    let mut c = COMPOSITOR.lock();
    for i in 0..c.window_count as usize {
        if c.windows[i].id == win_id {
            c.windows[i].bounds.x = x;
            c.windows[i].bounds.y = y;
            c.windows[i].needs_redraw = true;
            c.dirty = true;
            break;
        }
    }
}

/// Resize a window
pub fn window_resize(win_id: u32, w: u32, h: u32) {
    let mut c = COMPOSITOR.lock();
    for i in 0..c.window_count as usize {
        if c.windows[i].id == win_id {
            c.windows[i].bounds.w = core::cmp::max(w, MIN_WIDTH);
            c.windows[i].bounds.h = core::cmp::max(h, MIN_HEIGHT);
            c.windows[i].needs_redraw = true;
            c.dirty = true;
            break;
        }
    }
}

/// Focus a window
pub fn window_focus(win_id: u32) {
    let mut c = COMPOSITOR.lock();
    // Raise z-order
    c.z_counter += 1;
    for i in 0..c.window_count as usize {
        if c.windows[i].id == win_id {
            c.windows[i].z_order = c.z_counter;
            c.windows[i].state = WindowState::Focused;
            c.focused_window = win_id;
            c.dirty = true;
        } else if c.windows[i].state == WindowState::Focused {
            c.windows[i].state = WindowState::Visible;
        }
    }
}

/// Post an event to the compositor queue
pub fn event_post(event: Event) {
    let mut c = COMPOSITOR.lock();
    let next = (c.event_head + 1) % 256;
    if next != c.event_tail {
        let head = c.event_head as usize;
        c.event_queue[head] = event;
        c.event_head = next;
    }
}

/// Poll next event (returns None if empty)
pub fn event_poll() -> Option<Event> {
    let mut c = COMPOSITOR.lock();
    if c.event_head == c.event_tail { return None; }
    let ev = c.event_queue[c.event_tail as usize];
    c.event_tail = (c.event_tail + 1) % 256;
    Some(ev)
}

/// Handle mouse event
pub fn handle_mouse(x: i32, y: i32, buttons: u8) {
    let mut mouse = MOUSE_POS.lock();
    mouse.0 = x;
    mouse.1 = y;
    let new_lbutton = buttons & 1 != 0;

    // Post move event
    event_post(Event { kind: EventType::MouseMove, data1: x as u32, data2: y as u32, data3: buttons as u32, window_id: 0 });

    let mut c = COMPOSITOR.lock();

    // Left button click: find topmost window under cursor
    if new_lbutton && !mouse.2 {
        for i in (0..c.window_count as usize).rev() {
            let win = &c.windows[i];
            if win.state == WindowState::Hidden || win.state == WindowState::Minimized { continue; }
            let titlebar = Rect::new(
                win.bounds.x, win.bounds.y,
                win.bounds.w, TITLEBAR_HEIGHT,
            );
            let client = Rect::new(
                win.bounds.x, win.bounds.y + TITLEBAR_HEIGHT as i32,
                win.bounds.w, win.bounds.h.saturating_sub(TITLEBAR_HEIGHT),
            );

            if client.contains(x, y) {
                let wid = win.id;
                drop(c);
                drop(mouse);
                window_focus(wid);
                event_post(Event { kind: EventType::MouseDown, data1: (x - client.x) as u32, data2: (y - client.y) as u32, data3: 1, window_id: wid });
                return;
            }
            if titlebar.contains(x, y) {
                let wid = win.id;
                mouse.3 = wid;
                mouse.4 = x - win.bounds.x;
                mouse.5 = y - win.bounds.y;
                drop(c);
                drop(mouse);
                window_focus(wid);
                return;
            }
        }
    }

    // Dragging
    if new_lbutton && mouse.3 != 0 {
        for i in 0..c.window_count as usize {
            if c.windows[i].id == mouse.3 {
                c.windows[i].bounds.x = x - mouse.4;
                c.windows[i].bounds.y = y - mouse.5;
                c.windows[i].needs_redraw = true;
                c.dirty = true;
                break;
            }
        }
    }

    // Release drag
    if !new_lbutton && mouse.2 {
        mouse.3 = 0;
    }

    mouse.2 = new_lbutton;
}

/// Render the full desktop to the screen framebuffer
pub unsafe fn compositor_render() {
    let mut c = COMPOSITOR.lock();
    if !c.dirty { return; }
    c.dirty = false;

    let fb = crate::framebuffer::krust_framebuffer_get_fb_ptr();
    if fb.is_null() { return; }
    let info = crate::framebuffer::krust_framebuffer_info();
    let pitch = info.pitch;
    let bpp = info.bpp;
    let sw = info.width;

    // Clear desktop
    fb_fill_rect(0, 0, sw, c.screen_h, COLOR_DESKTOP, fb, sw, pitch, bpp);

    // Render windows in z-order
    for i in 0..c.window_count as usize {
        let win = &c.windows[i];
        if win.state == WindowState::Hidden || win.state == WindowState::Minimized { continue; }

        let bx = win.bounds.x;
        let by = win.bounds.y;
        let bw = win.bounds.w;
        let bh = win.bounds.h;

        fb_fill_rect(bx - BORDER_SIZE as i32, by - BORDER_SIZE as i32,
            bw + BORDER_SIZE * 2, bh + BORDER_SIZE * 2,
            COLOR_WINDOW_BORDER, fb, sw, pitch, bpp);

        fb_fill_rect(bx, by, bw, bh, COLOR_WINDOW_BG, fb, sw, pitch, bpp);

        let tb_color = if win.state == WindowState::Focused { COLOR_TITLEBAR_ACTIVE } else { COLOR_TITLEBAR };
        fb_fill_rect(bx, by, bw, TITLEBAR_HEIGHT, tb_color, fb, sw, pitch, bpp);

        let title = win.title_str();
        if title.len() > 0 {
            fb_draw_string((bx + 4) as u32, (by + 4) as u32, title.as_bytes(), COLOR_TITLEBAR_TEXT, fb, sw, pitch, bpp);
        }

        fb_fill_rect(bx + bw as i32 - 20, by + 2, 16, 16, COLOR_CLOSE_BTN, fb, sw, pitch, bpp);

        if !win.fb.is_null() {
            let content_y = by + TITLEBAR_HEIGHT as i32;
            let content_h = bh.saturating_sub(TITLEBAR_HEIGHT);
            let copy_w = core::cmp::min(bw, win.fb_w);
            let copy_h = core::cmp::min(content_h, win.fb_h);

            if bpp == 32 {
                let screen_pitch = pitch as usize / 4;
                for row in 0..copy_h {
                    let sy = content_y + row as i32;
                    if sy < 0 || sy >= c.screen_h as i32 { continue; }
                    for col in 0..copy_w {
                        let sx = bx + col as i32;
                        if sx < 0 || sx >= sw as i32 { continue; }
                        let src_idx = (row as usize) * (win.fb_pitch as usize / 4) + col as usize;
                        let dst_idx = sy as usize * screen_pitch + sx as usize;
                        let pixel = core::ptr::read_volatile(win.fb.add(src_idx * 4) as *const u32);
                        core::ptr::write_volatile(fb.add(dst_idx * 4) as *mut u32, pixel);
                    }
                }
            }
        }
    }

    // Taskbar
    let taskbar_y = c.screen_h - TASKBAR_HEIGHT;
    fb_fill_rect(0, taskbar_y as i32, sw, TASKBAR_HEIGHT, COLOR_TASKBAR, fb, sw, pitch, bpp);

    let mut btn_x: i32 = 4;
    for i in 0..c.window_count as usize {
        let win = &c.windows[i];
        if win.state == WindowState::Hidden { continue; }

        let btn_w: i32 = 100;
        let btn_color = if win.id == c.focused_window { COLOR_TITLEBAR_ACTIVE } else { 0x222244 };
        fb_fill_rect(btn_x, taskbar_y as i32 + 4, btn_w as u32, TASKBAR_HEIGHT - 8, btn_color, fb, sw, pitch, bpp);

        let title = win.title_str();
        if title.len() > 0 {
            fb_draw_string((btn_x + 4) as u32, (taskbar_y as i32 + 10) as u32, title.as_bytes(), COLOR_TITLEBAR_TEXT, fb, sw, pitch, bpp);
        }

        btn_x += btn_w + 4;
    }

    // Mouse cursor
    let (mx, my, _, _, _, _) = *MOUSE_POS.lock();
    render_cursor(mx, my, fb, sw, pitch, bpp);
}

unsafe fn render_cursor(x: i32, y: i32, fb: *mut u8, sw: u32, pitch: u32, bpp: u8) {
    // Simple 8x8 arrow cursor
    const CURSOR: [u8; 8] = [
        0b10000000,
        0b11000000,
        0b11100000,
        0b11110000,
        0b11111000,
        0b11100000,
        0b10110000,
        0b10011000,
    ];

    for row in 0..8u32 {
        for col in 0..8u32 {
            if CURSOR[row as usize] & (0x80 >> col) != 0 {
                let px = x + col as i32;
                let py = y + row as i32;
                if px >= 0 && py >= 0 && (px as u32) < sw && (py as u32) < pitch / (bpp as u32 / 8) {
                    fb_put_pixel(px as u32, py as u32, COLOR_MOUSE, fb, pitch, bpp);
                }
            }
        }
    }
}

// ─── Query API ────────────────────────────────────────────────

pub fn window_count() -> u32 {
    COMPOSITOR.lock().window_count
}

pub fn focused_window_id() -> u32 {
    COMPOSITOR.lock().focused_window
}

pub fn window_get_bounds(win_id: u32) -> Option<Rect> {
    let c = COMPOSITOR.lock();
    for i in 0..c.window_count as usize {
        if c.windows[i].id == win_id {
            return Some(c.windows[i].bounds);
        }
    }
    None
}

pub fn window_get_fb(win_id: u32) -> Option<(*mut u8, u32, u32, u32)> {
    let c = COMPOSITOR.lock();
    for i in 0..c.window_count as usize {
        if c.windows[i].id == win_id {
            let w = &c.windows[i];
            return Some((w.fb, w.fb_pitch, w.fb_w, w.fb_h));
        }
    }
    None
}

pub fn window_mark_dirty(win_id: u32) {
    let mut c = COMPOSITOR.lock();
    for i in 0..c.window_count as usize {
        if c.windows[i].id == win_id {
            c.windows[i].needs_redraw = true;
            c.dirty = true;
            break;
        }
    }
}

pub fn mark_dirty() {
    COMPOSITOR.lock().dirty = true;
}
