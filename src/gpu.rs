use super::convention::Term;
use super::intf::{Flag, Intf};
use super::memory::Memory;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Eq, PartialEq)]
pub enum HdmaMode {
    // When using this transfer method, all data is transferred at once. The execution of the program is halted until
    // the transfer has completed. Note that the General Purpose DMA blindly attempts to copy the data, even if the
    // CD controller is currently accessing VRAM. So General Purpose DMA should be used only if the Display is disabled,
    // or during V-Blank, or (for rather short blocks) during H-Blank. The execution of the program continues when the
    // transfer has been completed, and FF55 then contains a value of FFh.
    Gdma,
    // The H-Blank DMA transfers 10h bytes of data during each H-Blank, ie. at LY=0-143, no data is transferred during
    // V-Blank (LY=144-153), but the transfer will then continue at LY=00. The execution of the program is halted
    // during the separate transfers, but the program execution continues during the 'spaces' between each data block.
    // Note that the program should not change the Destination VRAM bank (FF4F), or the Source ROM/RAM bank (in case
    // data is transferred from bankable memory) until the transfer has completed! (The transfer should be paused as
    // described below while the banks are switched) Reading from Register FF55 returns the remaining length (divided
    // by 10h, minus 1), a value of 0FFh indicates that the transfer has completed. It is also possible to terminate
    // an active H-Blank transfer by writing zero to Bit 7 of FF55. In that case reading from FF55 will return how many
    // $10 "blocks" remained (minus 1) in the lower 7 bits, but Bit 7 will be read as "1". Stopping the transfer
    // doesn't set HDMA1-4 to $FF.
    Hdma,
}

pub struct Hdma {
    // These two registers specify the address at which the transfer will read data from. Normally, this should be
    // either in ROM, SRAM or WRAM, thus either in range 0000-7FF0 or A000-DFF0. [Note : this has yet to be tested on
    // Echo RAM, OAM, FEXX, IO and HRAM]. Trying to specify a source address in VRAM will cause garbage to be copied.
    // The four lower bits of this address will be ignored and treated as 0.
    pub src: u16,
    // These two registers specify the address within 8000-9FF0 to which the data will be copied. Only bits 12-4 are
    // respected; others are ignored. The four lower bits of this address will be ignored and treated as 0.
    pub dst: u16,
    pub active: bool,
    pub mode: HdmaMode,
    pub remain: u8,
}

impl Hdma {
    pub fn power_up() -> Self {
        Self { src: 0x0000, dst: 0x8000, active: false, mode: HdmaMode::Gdma, remain: 0x00 }
    }
}

impl Memory for Hdma {
    fn get(&self, a: u16) -> u8 {
        match a {
            0xff51 => (self.src >> 8) as u8,
            0xff52 => self.src as u8,
            0xff53 => (self.dst >> 8) as u8,
            0xff54 => self.dst as u8,
            0xff55 => self.remain | if self.active { 0x00 } else { 0x80 },
            _ => panic!(""),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0xff51 => self.src = (u16::from(v) << 8) | (self.src & 0x00ff),
            0xff52 => self.src = (self.src & 0xff00) | u16::from(v & 0xf0),
            0xff53 => self.dst = 0x8000 | (u16::from(v & 0x1f) << 8) | (self.dst & 0x00ff),
            0xff54 => self.dst = (self.dst & 0xff00) | u16::from(v & 0xf0),
            0xff55 => {
                if self.active && self.mode == HdmaMode::Hdma {
                    if v & 0x80 == 0x00 {
                        self.active = false;
                    };
                    return;
                }
                self.active = true;
                self.remain = v & 0x7f;
                self.mode = if v & 0x80 != 0x00 { HdmaMode::Hdma } else { HdmaMode::Gdma };
            }
            _ => panic!(""),
        };
    }
}

// LCDC is the main LCD Control register. Its bits toggle what elements are displayed on the screen, and how.
pub struct Lcdc {
    data: u8,
}

#[rustfmt::skip]
impl Lcdc {
    pub fn power_up() -> Self {
        Self { data: 0b0100_1000 }
    }

    // LCDC.7 - LCD Display Enable
    // This bit controls whether the LCD is on and the PPU is active. Setting it to 0 turns both off, which grants
    // immediate and full access to VRAM, OAM, etc.
    fn bit7(&self) -> bool { self.data & 0b1000_0000 != 0x00 }

    // LCDC.6 - Window Tile Map Display Select
    // This bit controls which background map the Window uses for rendering. When it's reset, the $9800 tilemap is used,
    // otherwise it's the $9C00 one.
    fn bit6(&self) -> bool { self.data & 0b0100_0000 != 0x00 }

    // LCDC.5 - Window Display Enable
    // This bit controls whether the window shall be displayed or not. (TODO : what happens when toggling this
    // mid-scanline ?) This bit is overridden on DMG by bit 0 if that bit is reset.
    // Note that on CGB models, setting this bit to 0 then back to 1 mid-frame may cause the second write to be ignored.
    fn bit5(&self) -> bool { self.data & 0b0010_0000 != 0x00 }

    // LCDC.4 - BG & Window Tile Data Select
    // This bit controls which addressing mode the BG and Window use to pick tiles.
    // Sprites aren't affected by this, and will always use $8000 addressing mode.
    fn bit4(&self) -> bool { self.data & 0b0001_0000 != 0x00 }

    // LCDC.3 - BG Tile Map Display Select
    // This bit works similarly to bit 6: if the bit is reset, the BG uses tilemap $9800, otherwise tilemap $9C00.
    fn bit3(&self) -> bool { self.data & 0b0000_1000 != 0x00 }

    // LCDC.2 - OBJ Size
    // This bit controls the sprite size (1 tile or 2 stacked vertically).
    // Be cautious when changing this mid-frame from 8x8 to 8x16 : "remnants" of the sprites intended for 8x8 could
    // "leak" into the 8x16 zone and cause artifacts.
    fn bit2(&self) -> bool { self.data & 0b0000_0100 != 0x00 }

    // LCDC.1 - OBJ Display Enable
    // This bit toggles whether sprites are displayed or not.
    // This can be toggled mid-frame, for example to avoid sprites being displayed on top of a status bar or text box.
    // (Note: toggling mid-scanline might have funky results on DMG? Investigation needed.)
    fn bit1(&self) -> bool { self.data & 0b0000_0010 != 0x00 }


    // LCDC.0 - BG/Window Display/Priority
    // LCDC.0 has different meanings depending on Gameboy type and Mode:
    // Monochrome Gameboy, SGB and CGB in Non-CGB Mode: BG Display
    // When Bit 0 is cleared, both background and window become blank (white), and the Window Display Bit is ignored in
    // that case. Only Sprites may still be displayed (if enabled in Bit 1).
    // CGB in CGB Mode: BG and Window Master Priority
    // When Bit 0 is cleared, the background and window lose their priority - the sprites will be always displayed on
    // top of background and window, independently of the priority flags in OAM and BG Map attributes.
    fn bit0(&self) -> bool { self.data & 0b0000_0001 != 0x00 }
}

// LCD Status Register.
pub struct Stat {
    // Bit 6 - LYC=LY Coincidence Interrupt (1=Enable) (Read/Write)
    enable_ly_interrupt: bool,
    // Bit 5 - Mode 2 OAM Interrupt         (1=Enable) (Read/Write)
    enable_m2_interrupt: bool,
    // Bit 4 - Mode 1 V-Blank Interrupt     (1=Enable) (Read/Write)
    enable_m1_interrupt: bool,
    // Bit 3 - Mode 0 H-Blank Interrupt     (1=Enable) (Read/Write)
    enable_m0_interrupt: bool,
    // Bit 1-0 - Mode Flag       (Mode 0-3, see below) (Read Only)
    //    0: During H-Blank
    //    1: During V-Blank
    //    2: During Searching OAM
    //    3: During Transferring Data to LCD Driver
    mode: u8,
}

impl Stat {
    pub fn power_up() -> Self {
        Self {
            enable_ly_interrupt: false,
            enable_m2_interrupt: false,
            enable_m1_interrupt: false,
            enable_m0_interrupt: false,
            mode: 0x00,
        }
    }
}

// This register is used to address a byte in the CGBs Background Palette Memory. Each two byte in that memory define a
// color value. The first 8 bytes define Color 0-3 of Palette 0 (BGP0), and so on for BGP1-7.
//  Bit 0-5   Index (00-3F)
//  Bit 7     Auto Increment  (0=Disabled, 1=Increment after Writing)
// Data can be read/written to/from the specified index address through Register FF69. When the Auto Increment bit is
// set then the index is automatically incremented after each <write> to FF69. Auto Increment has no effect when
// <reading> from FF69, so the index must be manually incremented in that case. Writing to FF69 during rendering still
// causes auto-increment to occur.
// Unlike the following, this register can be accessed outside V-Blank and H-Blank.
struct Bgpi {
    i: u8,
    auto_increment: bool,
}

impl Bgpi {
    fn power_up() -> Self {
        Self { i: 0x00, auto_increment: false }
    }

    fn get(&self) -> u8 {
        let a = if self.auto_increment { 0x80 } else { 0x00 };
        a | self.i
    }

    fn set(&mut self, v: u8) {
        self.auto_increment = v & 0x80 != 0x00;
        self.i = v & 0x3f;
    }
}

pub enum GrayShades {
    White = 0xff,
    Light = 0xc0,
    Dark = 0x60,
    Black = 0x00,
}

// Bit7   OBJ-to-BG Priority (0=OBJ Above BG, 1=OBJ Behind BG color 1-3)
//     (Used for both BG and Window. BG color 0 is always behind OBJ)
// Bit6   Y flip          (0=Normal, 1=Vertically mirrored)
// Bit5   X flip          (0=Normal, 1=Horizontally mirrored)
// Bit4   Palette number  **Non CGB Mode Only** (0=OBP0, 1=OBP1)
// Bit3   Tile VRAM-Bank  **CGB Mode Only**     (0=Bank 0, 1=Bank 1)
// Bit2-0 Palette number  **CGB Mode Only**     (OBP0-7)
struct Attr {
    priority: bool,
    yflip: bool,
    xflip: bool,
    palette_number_0: usize,
    bank: bool,
    palette_number_1: usize,
}

impl From<u8> for Attr {
    fn from(u: u8) -> Self {
        Self {
            priority: u & (1 << 7) != 0,
            yflip: u & (1 << 6) != 0,
            xflip: u & (1 << 5) != 0,
            palette_number_0: u as usize & (1 << 4),
            bank: u & (1 << 3) != 0,
            palette_number_1: u as usize & 0x07,
        }
    }
}

pub const SCREEN_W: usize = 160;
pub const SCREEN_H: usize = 144;

pub struct Gpu {
    // Digital image with mode RGB. Size = 144 * 160 * 3.
    // 3---------
    // ----------
    // ----------
    // ---------- 160
    //        144
    pub data: [[[u8; 3]; SCREEN_W]; SCREEN_H],
    pub intf: Rc<RefCell<Intf>>,
    pub term: Term,
    pub h_blank: bool,
    pub v_blank: bool,

    lcdc: Lcdc,
    stat: Stat,
    // Scroll Y (R/W), Scroll X (R/W)
    // Specifies the position in the 256x256 pixels BG map (32x32 tiles) which is to be displayed at the upper/left LCD
    // display position. Values in range from 0-255 may be used for X/Y each, the video controller automatically wraps
    // back to the upper (left) position in BG map when drawing exceeds the lower (right) border of the BG map area.
    sy: u8,
    sx: u8,
    // Window Y Position (R/W), Window X Position minus 7 (R/W)
    wy: u8,
    wx: u8,
    // The LY indicates the vertical line to which the present data is transferred to the LCD Driver. The LY can take
    // on any value between 0 through 153. The values between 144 and 153 indicate the V-Blank period. Writing will
    // reset the counter.
    ly: u8,
    // The Gameboy permanently compares the value of the LYC and LY registers. When both values are identical, the
    // coincident bit in the STAT register becomes set, and (if enabled) a STAT interrupt is requested.
    lc: u8,

    // This register assigns gray shades to the color numbers of the BG and Window tiles.
    bgp: u8,
    // This register assigns gray shades for sprite palette 0. It works exactly as BGP (FF47), except that the lower
    // two bits aren't used because sprite data 00 is transparent.
    op0: u8,
    // This register assigns gray shades for sprite palette 1. It works exactly as BGP (FF47), except that the lower
    // two bits aren't used because sprite data 00 is transparent.
    op1: u8,

    cbgpi: Bgpi,
    // This register allows to read/write data to the CGBs Background Palette Memory, addressed through Register FF68.
    // Each color is defined by two bytes (Bit 0-7 in first byte).
    //     Bit 0-4   Red Intensity   (00-1F)
    //     Bit 5-9   Green Intensity (00-1F)
    //     Bit 10-14 Blue Intensity  (00-1F)
    // Much like VRAM, data in Palette Memory cannot be read/written during the time when the LCD Controller is
    // reading from it. (That is when the STAT register indicates Mode 3). Note: All background colors are initialized
    // as white by the boot ROM, but it's a good idea to initialize at least one color yourself (for example if you
    // include a soft-reset mechanic).
    //
    // Note: Type [[[u8; 3]; 4]; 8] equals with [u8; 64].
    cbgpd: [[[u8; 3]; 4]; 8],

    cobpi: Bgpi,
    cobpd: [[[u8; 3]; 4]; 8],

    ram: [u8; 0x4000],
    ram_bank: usize,
    // VRAM Sprite Attribute Table (OAM)
    // Gameboy video controller can display up to 40 sprites either in 8x8 or in 8x16 pixels. Because of a limitation of
    // hardware, only ten sprites can be displayed per scan line. Sprite patterns have the same format as BG tiles, but
    // they are taken from the Sprite Pattern Table located at $8000-8FFF and have unsigned numbering.
    // Sprite attributes reside in the Sprite Attribute Table (OAM - Object Attribute Memory) at $FE00-FE9F. Each of the 40
    // entries consists of four bytes with the following meanings:
    // Byte0 - Y Position
    // Specifies the sprites vertical position on the screen (minus 16). An off-screen value (for example, Y=0 or
    // Y>=160) hides the sprite.
    //
    // Byte1 - X Position
    // Specifies the sprites horizontal position on the screen (minus 8). An off-screen value (X=0 or X>=168) hides the
    // sprite, but the sprite still affects the priority ordering - a better way to hide a sprite is to set its
    // Y-coordinate off-screen.
    //
    // Byte2 - Tile/Pattern Number
    // Specifies the sprites Tile Number (00-FF). This (unsigned) value selects a tile from memory at 8000h-8FFFh. In
    // CGB Mode this could be either in VRAM Bank 0 or 1, depending on Bit 3 of the following byte. In 8x16 mode, the
    // lower bit of the tile number is ignored. IE: the upper 8x8 tile is "NN AND FEh", and the lower 8x8 tile
    // is "NN OR 01h".
    //
    // Byte3 - Attributes/Flags:
    // Bit7   OBJ-to-BG Priority (0=OBJ Above BG, 1=OBJ Behind BG color 1-3)
    //        (Used for both BG and Window. BG color 0 is always behind OBJ)
    // Bit6   Y flip          (0=Normal, 1=Vertically mirrored)
    // Bit5   X flip          (0=Normal, 1=Horizontally mirrored)
    // Bit4   Palette number  **Non CGB Mode Only** (0=OBP0, 1=OBP1)
    // Bit3   Tile VRAM-Bank  **CGB Mode Only**     (0=Bank 0, 1=Bank 1)
    // Bit2-0 Palette number  **CGB Mode Only**     (OBP0-7)
    oam: [u8; 0xa0],

    prio: [(bool, usize); SCREEN_W],
    // The LCD controller operates on a 222 Hz = 4.194 MHz dot clock. An entire frame is 154 scanlines, 70224 dots, or
    // 16.74 ms. On scanlines 0 through 143, the LCD controller cycles through modes 2, 3, and 0 once every 456 dots.
    // Scanlines 144 through 153 are mode 1.
    dots: u32,
}

impl Gpu {
    pub fn power_up(term: Term, intf: Rc<RefCell<Intf>>) -> Self {
        Self {
            data: [[[0xffu8; 3]; SCREEN_W]; SCREEN_H],
            intf,
            term,
            h_blank: false,
            v_blank: false,

            lcdc: Lcdc::power_up(),
            stat: Stat::power_up(),
            sy: 0x00,
            sx: 0x00,
            wx: 0x00,
            wy: 0x00,
            ly: 0x00,
            lc: 0x00,
            bgp: 0x00,
            op0: 0x00,
            op1: 0x01,
            cbgpi: Bgpi::power_up(),
            cbgpd: [[[0u8; 3]; 4]; 8],
            cobpi: Bgpi::power_up(),
            cobpd: [[[0u8; 3]; 4]; 8],
            ram: [0x00; 0x4000],
            ram_bank: 0x00,
            oam: [0x00; 0xa0],
            prio: [(true, 0); SCREEN_W],
            dots: 0,
        }
    }

    fn get_ram0(&self, a: u16) -> u8 {
        self.ram[a as usize - 0x8000]
    }

    fn get_ram1(&self, a: u16) -> u8 {
        self.ram[a as usize - 0x6000]
    }

    // This register assigns gray shades to the color numbers of the BG and Window tiles.
    // Bit 7-6 - Shade for Color Number 3
    // Bit 5-4 - Shade for Color Number 2
    // Bit 3-2 - Shade for Color Number 1
    // Bit 1-0 - Shade for Color Number 0
    // The four possible gray shades are:
    // 0  White
    // 1  Light gray
    // 2  Dark gray
    // 3  Black
    fn get_gray_shades(v: u8, i: usize) -> GrayShades {
        match v >> (2 * i) & 0x03 {
            0x00 => GrayShades::White,
            0x01 => GrayShades::Light,
            0x02 => GrayShades::Dark,
            _ => GrayShades::Black,
        }
    }

    // Grey scale.
    fn set_gre(&mut self, x: usize, g: u8) {
        self.data[self.ly as usize][x] = [g, g, g];
    }

    // When developing graphics on PCs, note that the RGB values will have different appearance on CGB displays as on
    // VGA/HDMI monitors calibrated to sRGB color. Because the GBC is not lit, the highest intensity will produce Light
    // Gray color rather than White. The intensities are not linear; the values 10h-1Fh will all appear very bright,
    // while medium and darker colors are ranged at 00h-0Fh.
    // The CGB display's pigments aren't perfectly saturated. This means the colors mix quite oddly; increasing
    // intensity of only one R,G,B color will also influence the other two R,G,B colors. For example, a color setting
    // of 03EFh (Blue=0, Green=1Fh, Red=0Fh) will appear as Neon Green on VGA displays, but on the CGB it'll produce a
    // decently washed out Yellow. See image on the right.
    fn set_rgb(&mut self, x: usize, r: u8, g: u8, b: u8) {
        assert!(r <= 0x1f);
        assert!(g <= 0x1f);
        assert!(b <= 0x1f);
        let r = u32::from(r);
        let g = u32::from(g);
        let b = u32::from(b);
        let lr = ((r * 13 + g * 2 + b) >> 1) as u8;
        let lg = ((g * 3 + b) << 1) as u8;
        let lb = ((r * 3 + g * 2 + b * 11) >> 1) as u8;
        self.data[self.ly as usize][x] = [lr, lg, lb];
    }

    pub fn next(&mut self, cycles: u32) {
        if !self.lcdc.bit7() {
            return;
        }
        self.h_blank = false;

        // The LCD controller operates on a 222 Hz = 4.194 MHz dot clock. An entire frame is 154 scanlines, 70224 dots,
        // or 16.74 ms. On scanlines 0 through 143, the LCD controller cycles through modes 2, 3, and 0 once every 456
        // dots. Scanlines 144 through 153 are mode 1.
        //
        // 1 scanline = 456 dots
        //
        // The following are typical when the display is enabled:
        // Mode 2  2_____2_____2_____2_____2_____2___________________2____
        // Mode 3  _33____33____33____33____33____33__________________3___
        // Mode 0  ___000___000___000___000___000___000________________000
        // Mode 1  ____________________________________11111111111111_____
        if cycles == 0 {
            return;
        }
        let c = (cycles - 1) / 80 + 1;
        for i in 0..c {
            if i == (c - 1) {
                self.dots += cycles % 80
            } else {
                self.dots += 80
            }
            let d = self.dots;
            self.dots %= 456;
            if d != self.dots {
                self.ly = (self.ly + 1) % 154;
                if self.stat.enable_ly_interrupt && self.ly == self.lc {
                    self.intf.borrow_mut().hi(Flag::LCDStat);
                }
            }
            if self.ly >= 144 {
                if self.stat.mode == 1 {
                    continue;
                }
                self.stat.mode = 1;
                self.v_blank = true;
                self.intf.borrow_mut().hi(Flag::VBlank);
                if self.stat.enable_m1_interrupt {
                    self.intf.borrow_mut().hi(Flag::LCDStat);
                }
            } else if self.dots <= 80 {
                if self.stat.mode == 2 {
                    continue;
                }
                self.stat.mode = 2;
                if self.stat.enable_m2_interrupt {
                    self.intf.borrow_mut().hi(Flag::LCDStat);
                }
            } else if self.dots <= (80 + 172) {
                self.stat.mode = 3;
            } else {
                if self.stat.mode == 0 {
                    continue;
                }
                self.stat.mode = 0;
                self.h_blank = true;
                if self.stat.enable_m0_interrupt {
                    self.intf.borrow_mut().hi(Flag::LCDStat);
                }
                // Render scanline
                if self.term == Term::GBC || self.lcdc.bit0() {
                    self.draw_bg();
                }
                if self.lcdc.bit1() {
                    self.draw_sprites();
                }
            }
        }
    }

    fn draw_bg(&mut self) {
        let show_window = self.lcdc.bit5() && self.wy <= self.ly;
        let tile_base = if self.lcdc.bit4() { 0x8000 } else { 0x8800 };

        let wx = self.wx.wrapping_sub(7);
        let py = if show_window { self.ly.wrapping_sub(self.wy) } else { self.sy.wrapping_add(self.ly) };
        let ty = (u16::from(py) >> 3) & 31;

        for x in 0..SCREEN_W {
            let px = if show_window && x as u8 >= wx { x as u8 - wx } else { self.sx.wrapping_add(x as u8) };
            let tx = (u16::from(px) >> 3) & 31;

            // Background memory base addr.
            let bg_base = if show_window && x as u8 >= wx {
                if self.lcdc.bit6() { 0x9c00 } else { 0x9800 }
            } else if self.lcdc.bit3() {
                0x9c00
            } else {
                0x9800
            };

            // Tile data
            // Each tile is sized 8x8 pixels and has a color depth of 4 colors/gray shades.
            // Each tile occupies 16 bytes, where each 2 bytes represent a line:
            // Byte 0-1  First Line (Upper 8 pixels)
            // Byte 2-3  Next Line
            // etc.
            let tile_addr = bg_base + ty * 32 + tx;
            let tile_number = self.get_ram0(tile_addr);
            let tile_offset =
                if self.lcdc.bit4() { i16::from(tile_number) } else { i16::from(tile_number as i8) + 128 } as u16 * 16;
            let tile_location = tile_base + tile_offset;
            let tile_attr = Attr::from(self.get_ram1(tile_addr));

            let tile_y = if tile_attr.yflip { 7 - py % 8 } else { py % 8 };
            let tile_y_data: [u8; 2] = if self.term == Term::GBC && tile_attr.bank {
                let a = self.get_ram1(tile_location + u16::from(tile_y * 2));
                let b = self.get_ram1(tile_location + u16::from(tile_y * 2) + 1);
                [a, b]
            } else {
                let a = self.get_ram0(tile_location + u16::from(tile_y * 2));
                let b = self.get_ram0(tile_location + u16::from(tile_y * 2) + 1);
                [a, b]
            };
            let tile_x = if tile_attr.xflip { 7 - px % 8 } else { px % 8 };

            // Palettes
            let color_l = if tile_y_data[0] & (0x80 >> tile_x) != 0 { 1 } else { 0 };
            let color_h = if tile_y_data[1] & (0x80 >> tile_x) != 0 { 2 } else { 0 };
            let color = color_h | color_l;

            // Priority
            self.prio[x] = (tile_attr.priority, color);

            if self.term == Term::GBC {
                let r = self.cbgpd[tile_attr.palette_number_1][color][0];
                let g = self.cbgpd[tile_attr.palette_number_1][color][1];
                let b = self.cbgpd[tile_attr.palette_number_1][color][2];
                self.set_rgb(x as usize, r, g, b);
            } else {
                let color = Self::get_gray_shades(self.bgp, color) as u8;
                self.set_gre(x, color);
            }
        }
    }

    // Gameboy video controller can display up to 40 sprites either in 8x8 or in 8x16 pixels. Because of a limitation
    // of hardware, only ten sprites can be displayed per scan line. Sprite patterns have the same format as BG tiles,
    // but they are taken from the Sprite Pattern Table located at $8000-8FFF and have unsigned numbering.
    //
    // Sprite attributes reside in the Sprite Attribute Table (OAM - Object Attribute Memory) at $FE00-FE9F. Each of
    // the 40 entries consists of four bytes with the following meanings:
    //   Byte0 - Y Position
    //   Specifies the sprites vertical position on the screen (minus 16). An off-screen value (for example, Y=0 or
    //   Y>=160) hides the sprite.
    //
    //   Byte1 - X Position
    //   Specifies the sprites horizontal position on the screen (minus 8). An off-screen value (X=0 or X>=168) hides
    //   the sprite, but the sprite still affects the priority ordering - a better way to hide a sprite is to set its
    //   Y-coordinate off-screen.
    //
    //   Byte2 - Tile/Pattern Number
    //   Specifies the sprites Tile Number (00-FF). This (unsigned) value selects a tile from memory at 8000h-8FFFh. In
    //   CGB Mode this could be either in VRAM Bank 0 or 1, depending on Bit 3 of the following byte. In 8x16 mode, the
    //   lower bit of the tile number is ignored. IE: the upper 8x8 tile is "NN AND FEh", and the lower 8x8 tile is
    //   "NN OR 01h".
    //
    //   Byte3 - Attributes/Flags:
    //     Bit7   OBJ-to-BG Priority (0=OBJ Above BG, 1=OBJ Behind BG color 1-3)
    //           (Used for both BG and Window. BG color 0 is always behind OBJ)
    //     Bit6   Y flip          (0=Normal, 1=Vertically mirrored)
    //     Bit5   X flip          (0=Normal, 1=Horizontally mirrored)
    //     Bit4   Palette number  **Non CGB Mode Only** (0=OBP0, 1=OBP1)
    //     Bit3   Tile VRAM-Bank  **CGB Mode Only**     (0=Bank 0, 1=Bank 1)
    //     Bit2-0 Palette number  **CGB Mode Only**     (OBP0-7)
    fn draw_sprites(&mut self) {
        // Sprite tile size 8x8 or 8x16(2 stacked vertically).
        let sprite_size = if self.lcdc.bit2() { 16 } else { 8 };
        for i in 0..40 {
            let sprite_addr = 0xfe00 + (i as u16) * 4;
            let py = self.get(sprite_addr).wrapping_sub(16);
            let px = self.get(sprite_addr + 1).wrapping_sub(8);
            let tile_number = self.get(sprite_addr + 2) & if self.lcdc.bit2() { 0xfe } else { 0xff };
            let tile_attr = Attr::from(self.get(sprite_addr + 3));

            // If this is true the scanline is out of the area we care about
            if py <= 0xff - sprite_size + 1 {
                if self.ly < py || self.ly > py + sprite_size - 1 {
                    continue;
                }
            } else {
                if self.ly > py.wrapping_add(sprite_size) - 1 {
                    continue;
                }
            }
            if px >= (SCREEN_W as u8) && px <= (0xff - 7) {
                continue;
            }

            let tile_y =
                if tile_attr.yflip { sprite_size - 1 - self.ly.wrapping_sub(py) } else { self.ly.wrapping_sub(py) };
            let tile_y_addr = 0x8000u16 + u16::from(tile_number) * 16 + u16::from(tile_y) * 2;
            let tile_y_data: [u8; 2] = if self.term == Term::GBC && tile_attr.bank {
                let b1 = self.get_ram1(tile_y_addr);
                let b2 = self.get_ram1(tile_y_addr + 1);
                [b1, b2]
            } else {
                let b1 = self.get_ram0(tile_y_addr);
                let b2 = self.get_ram0(tile_y_addr + 1);
                [b1, b2]
            };

            for x in 0..8 {
                if px.wrapping_add(x) >= (SCREEN_W as u8) {
                    continue;
                }
                let tile_x = if tile_attr.xflip { 7 - x } else { x };

                // Palettes
                let color_l = if tile_y_data[0] & (0x80 >> tile_x) != 0 { 1 } else { 0 };
                let color_h = if tile_y_data[1] & (0x80 >> tile_x) != 0 { 2 } else { 0 };
                let color = color_h | color_l;
                if color == 0 {
                    continue;
                }

                // Confirm the priority of background and sprite.
                let prio = self.prio[px.wrapping_add(x) as usize];
                let skip = if self.term == Term::GBC && !self.lcdc.bit0() {
                    prio.1 == 0
                } else if prio.0 {
                    prio.1 != 0
                } else {
                    tile_attr.priority && prio.1 != 0
                };
                if skip {
                    continue;
                }

                if self.term == Term::GBC {
                    let r = self.cobpd[tile_attr.palette_number_1][color][0];
                    let g = self.cobpd[tile_attr.palette_number_1][color][1];
                    let b = self.cobpd[tile_attr.palette_number_1][color][2];
                    self.set_rgb(px.wrapping_add(x) as usize, r, g, b);
                } else {
                    let color = if tile_attr.palette_number_0 == 1 {
                        Self::get_gray_shades(self.op1, color) as u8
                    } else {
                        Self::get_gray_shades(self.op0, color) as u8
                    };
                    self.set_gre(px.wrapping_add(x) as usize, color);
                }
            }
        }
    }
}

impl Memory for Gpu {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x8000..=0x9fff => self.ram[self.ram_bank * 0x2000 + a as usize - 0x8000],
            0xfe00..=0xfe9f => self.oam[a as usize - 0xfe00],
            0xff40 => self.lcdc.data,
            0xff41 => {
                let bit6 = if self.stat.enable_ly_interrupt { 0x40 } else { 0x00 };
                let bit5 = if self.stat.enable_m2_interrupt { 0x20 } else { 0x00 };
                let bit4 = if self.stat.enable_m1_interrupt { 0x10 } else { 0x00 };
                let bit3 = if self.stat.enable_m0_interrupt { 0x08 } else { 0x00 };
                let bit2 = if self.ly == self.lc { 0x04 } else { 0x00 };
                bit6 | bit5 | bit4 | bit3 | bit2 | self.stat.mode
            }
            0xff42 => self.sy,
            0xff43 => self.sx,
            0xff44 => self.ly,
            0xff45 => self.lc,
            0xff47 => self.bgp,
            0xff48 => self.op0,
            0xff49 => self.op1,
            0xff4a => self.wy,
            0xff4b => self.wx,
            0xff4f => 0xfe | self.ram_bank as u8,
            0xff68 => self.cbgpi.get(),
            0xff69 => {
                let r = self.cbgpi.i as usize >> 3;
                let c = self.cbgpi.i as usize >> 1 & 0x3;
                if self.cbgpi.i & 0x01 == 0x00 {
                    let a = self.cbgpd[r][c][0];
                    let b = self.cbgpd[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.cbgpd[r][c][1] >> 3;
                    let b = self.cbgpd[r][c][2] << 2;
                    a | b
                }
            }
            0xff6a => self.cobpi.get(),
            0xff6b => {
                let r = self.cobpi.i as usize >> 3;
                let c = self.cobpi.i as usize >> 1 & 0x3;
                if self.cobpi.i & 0x01 == 0x00 {
                    let a = self.cobpd[r][c][0];
                    let b = self.cobpd[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.cobpd[r][c][1] >> 3;
                    let b = self.cobpd[r][c][2] << 2;
                    a | b
                }
            }
            _ => panic!(""),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x8000..=0x9fff => self.ram[self.ram_bank * 0x2000 + a as usize - 0x8000] = v,
            0xfe00..=0xfe9f => self.oam[a as usize - 0xfe00] = v,
            0xff40 => {
                self.lcdc.data = v;
                if !self.lcdc.bit7() {
                    self.dots = 0;
                    self.ly = 0;
                    self.stat.mode = 0;
                    // Clean screen.
                    self.data = [[[0xffu8; 3]; SCREEN_W]; SCREEN_H];
                    self.v_blank = true;
                }
            }
            0xff41 => {
                self.stat.enable_ly_interrupt = v & 0x40 != 0x00;
                self.stat.enable_m2_interrupt = v & 0x20 != 0x00;
                self.stat.enable_m1_interrupt = v & 0x10 != 0x00;
                self.stat.enable_m0_interrupt = v & 0x08 != 0x00;
            }
            0xff42 => self.sy = v,
            0xff43 => self.sx = v,
            0xff44 => {}
            0xff45 => self.lc = v,
            0xff47 => self.bgp = v,
            0xff48 => self.op0 = v,
            0xff49 => self.op1 = v,
            0xff4a => self.wy = v,
            0xff4b => self.wx = v,
            0xff4f => self.ram_bank = (v & 0x01) as usize,
            0xff68 => self.cbgpi.set(v),
            0xff69 => {
                let r = self.cbgpi.i as usize >> 3;
                let c = self.cbgpi.i as usize >> 1 & 0x03;
                if self.cbgpi.i & 0x01 == 0x00 {
                    self.cbgpd[r][c][0] = v & 0x1f;
                    self.cbgpd[r][c][1] = (self.cbgpd[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.cbgpd[r][c][1] = (self.cbgpd[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.cbgpd[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.cbgpi.auto_increment {
                    self.cbgpi.i += 0x01;
                    self.cbgpi.i &= 0x3f;
                }
            }
            0xff6a => self.cobpi.set(v),
            0xff6b => {
                let r = self.cobpi.i as usize >> 3;
                let c = self.cobpi.i as usize >> 1 & 0x03;
                if self.cobpi.i & 0x01 == 0x00 {
                    self.cobpd[r][c][0] = v & 0x1f;
                    self.cobpd[r][c][1] = (self.cobpd[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.cobpd[r][c][1] = (self.cobpd[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.cobpd[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.cobpi.auto_increment {
                    self.cobpi.i += 0x01;
                    self.cobpi.i &= 0x3f;
                }
            }
            _ => panic!(""),
        }
    }
}
