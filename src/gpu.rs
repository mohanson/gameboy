use super::convention::Term;
use super::memory::Memory;

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
    pub data: [u8; 0x04],

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
        Self {
            data: [0x00; 0x04],
            src: 0x00,
            dst: 0x00,
            active: false,
            mode: HdmaMode::Gdma,
            remain: 0x00,
        }
    }
}

impl Memory for Hdma {
    fn get(&self, a: u16) -> u8 {
        match a {
            0xff51...0xff54 => self.data[(a - 0xff51) as usize],
            0xff55 => self.remain | if self.active { 0x00 } else { 0x80 },
            _ => panic!(""),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0xff51 => self.data[0] = v,
            0xff52 => self.data[1] = v & 0xf0,
            0xff53 => self.data[2] = v & 0x1F,
            0xff54 => self.data[3] = v & 0xf0,
            0xff55 => {
                if self.active && self.mode == HdmaMode::Hdma {
                    if v & 0x80 == 0x00 {
                        self.active = false;
                    };
                    return;
                }
                self.active = true;
                self.src = ((self.data[0] as u16) << 8) | (self.data[1] as u16);
                self.dst = ((self.data[2] as u16) << 8) | (self.data[3] as u16) | 0x8000;
                self.remain = v & 0x7f;
                self.mode = if v & 0x80 == 0x80 {
                    HdmaMode::Hdma
                } else {
                    HdmaMode::Gdma
                };
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
    // Be cautious when changing this mid-frame from 8x8 to 8x16 : "remnants" of the sprites intended for 8x8 could "leak" into the 8x16 zone and cause artifacts.
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

pub enum GrayShades {
    White = 0xff,
    Light = 0xc0,
    Dark = 0x60,
    Black = 0x00,
}

#[derive(PartialEq, Copy, Clone)]
enum PrioType {
    Priority,
    Zero,
    Else,
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
    pub blanked: bool,
    // Digital image with mode RGB. Size = 144 * 160 * 3.
    // 3---------
    // ----------
    // ----------
    // ---------- 160
    //        144
    pub data: [[[u8; 3]; SCREEN_W]; SCREEN_H],
    pub interrupt: u8,
    pub term: Term,
    pub updated: bool,

    // This register assigns gray shades to the color numbers of the BG and Window tiles.
    bgp: u8,
    bgprio: [PrioType; SCREEN_W],
    cbgpal_inc: bool,
    cbgpal_ind: u8,
    cbgpal: [[[u8; 3]; 4]; 8],
    csprit_inc: bool,
    csprit_ind: u8,
    csprit: [[[u8; 3]; 4]; 8],
    // The LCD controller operates on a 222 Hz = 4.194 MHz dot clock. An entire frame is 154 scanlines, 70224 dots, or
    // 16.74 ms. On scanlines 0 through 143, the LCD controller cycles through modes 2, 3, and 0 once every 456 dots.
    // Scanlines 144 through 153 are mode 1.
    dots: u32,
    lcdc: Lcdc,
    // The LY indicates the vertical line to which the present data is transferred to the LCD Driver. The LY can take
    // on any value between 0 through 153. The values between 144 and 153 indicate the V-Blank period. Writing will
    // reset the counter.
    ly: u8,
    // The Gameboy permanently compares the value of the LYC and LY registers. When both values are identical, the
    // coincident bit in the STAT register becomes set, and (if enabled) a STAT interrupt is requested.
    ly_compare: u8,
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
    // This register assigns gray shades for sprite palette 0. It works exactly as BGP (FF47), except that the lower
    // two bits aren't used because sprite data 00 is transparent.
    op0: u8,
    // This register assigns gray shades for sprite palette 1. It works exactly as BGP (FF47), except that the lower
    // two bits aren't used because sprite data 00 is transparent.
    op1: u8,
    ram: [[u8; 0x2000]; 0x02],
    ram_bank: usize,
    // Scroll Y (R/W), Scroll X (R/W)
    // Specifies the position in the 256x256 pixels BG map (32x32 tiles) which is to be displayed at the upper/left LCD
    // display position. Values in range from 0-255 may be used for X/Y each, the video controller automatically wraps
    // back to the upper (left) position in BG map when drawing exceeds the lower (right) border of the BG map area.
    sx: u8,
    sy: u8,
    stat: Stat,
    // Window Y Position (R/W), Window X Position minus 7 (R/W)
    wx: u8,
    wy: u8,
}

impl Gpu {
    pub fn power_up(term: Term) -> Self {
        Self {
            blanked: false,
            data: [[[0xffu8; 3]; SCREEN_W]; SCREEN_H],
            interrupt: 0,
            term: term,
            updated: false,

            bgp: 0x00,
            bgprio: [PrioType::Else; SCREEN_W],
            cbgpal_inc: false,
            cbgpal_ind: 0,
            cbgpal: [[[0u8; 3]; 4]; 8],
            csprit_inc: false,
            csprit_ind: 0,
            csprit: [[[0u8; 3]; 4]; 8],
            dots: 0,
            lcdc: Lcdc::power_up(),
            ly: 0x00,
            ly_compare: 0x00,
            oam: [0x00; 0xa0],
            op0: 0x00,
            op1: 0x01,
            ram: [[0x00; 0x2000]; 0x02],
            ram_bank: 0x00,
            sx: 0x00,
            sy: 0x00,
            stat: Stat::power_up(),
            wx: 0x00,
            wy: 0x00,
        }
    }

    fn get_ram0(&self, a: u16) -> u8 {
        self.ram[0][a as usize - 0x8000]
    }

    fn get_ram1(&self, a: u16) -> u8 {
        self.ram[1][a as usize - 0x8000]
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
        match (v >> 2 * i) & 0x03 {
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
        let r = r as u32;
        let g = g as u32;
        let b = b as u32;
        let lr = ((r * 13 + g * 2 + b) >> 1) as u8;
        let lg = ((g * 3 + b) << 1) as u8;
        let lb = ((r * 3 + g * 2 + b * 11) >> 1) as u8;
        self.data[self.ly as usize][x] = [lr, lg, lb];
    }

    pub fn next(&mut self, cycles: u32) {
        if !self.lcdc.bit7() {
            return;
        }
        self.blanked = false;

        // The LCD controller operates on a 222 Hz = 4.194 MHz dot clock. An entire frame is 154 scanlines, 70224 dots,
        // or 16.74 ms. On scanlines 0 through 143, the LCD controller cycles through modes 2, 3, and 0 once every 456
        // dots. Scanlines 144 through 153 are mode 1.
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
                if self.stat.enable_ly_interrupt && self.ly == self.ly_compare {
                    self.interrupt |= 0x02;
                }
            }
            // The following are typical when the display is enabled:
            // Mode 2  2_____2_____2_____2_____2_____2___________________2____
            // Mode 3  _33____33____33____33____33____33__________________3___
            // Mode 0  ___000___000___000___000___000___000________________000
            // Mode 1  ____________________________________11111111111111_____
            if self.ly >= 144 {
                self.ensure_mode(1);
            } else if self.dots <= 80 {
                self.ensure_mode(2);
            } else if self.dots <= (80 + 172) {
                self.ensure_mode(3);
            } else {
                self.ensure_mode(0);
            }
        }
    }

    fn ensure_mode(&mut self, mode: u8) {
        if self.stat.mode == mode {
            return;
        }
        self.stat.mode = mode;

        match self.stat.mode {
            0 => {
                self.render_scan();
                self.blanked = true;
                if self.stat.enable_m0_interrupt {
                    self.interrupt |= 0x02
                }
            }
            1 => {
                self.interrupt |= 0x01;
                self.updated = true;
                if self.stat.enable_m1_interrupt {
                    self.interrupt |= 0x02
                }
            }
            2 => {
                if self.stat.enable_m2_interrupt {
                    self.interrupt |= 0x02
                }
            }
            3 => {}
            _ => panic!(""),
        };
    }
}

impl Gpu {
    fn render_scan(&mut self) {
        for x in 0..SCREEN_W {
            self.set_gre(x, 0xff);
            self.bgprio[x] = PrioType::Else;
        }
        if self.lcdc.bit0() {
            self.draw_bg();
        }
        if self.lcdc.bit1() {
            self.draw_sprites();
        }
    }

    fn draw_bg(&mut self) {
        let using_window = self.lcdc.bit5() && self.wy <= self.ly;
        let tile_base = if self.lcdc.bit4() { 0x8000 } else { 0x8800 };

        let py = if using_window {
            self.ly.wrapping_sub(self.wy)
        } else {
            self.sy.wrapping_add(self.ly)
        };
        let ty = (py as u16 >> 3) & 31;

        for x in 0..SCREEN_W {
            // Translate the current x pos to window space if necessary
            let px = if using_window && x as u8 >= self.wx {
                x as u8 - self.wx
            } else {
                self.sx.wrapping_add(x as u8)
            };
            let tx = (px as u16 >> 3) & 31;

            let bg = if using_window && x as u8 >= self.wx {
                if self.lcdc.bit6() {
                    0x9c00
                } else {
                    0x9800
                }
            } else {
                if self.lcdc.bit3() {
                    0x9C00
                } else {
                    0x9800
                }
            };

            let tile_address = bg + ty * 32 + tx;
            let tile_num = self.get_ram0(tile_address);
            let tile_location = tile_base
                + (if self.lcdc.bit4() {
                    tile_num as u16
                } else {
                    (tile_num as i8 as i16 + 128) as u16
                }) * 16;
            let tile_attr = Attr::from(self.get_ram1(tile_address));

            let line = if self.term == Term::GBC && tile_attr.yflip {
                ((7u8.wrapping_sub(py)) % 8) * 2
            } else {
                (py % 8) * 2
            };
            let b1: u8;
            let b2: u8;
            if self.term == Term::GBC && tile_attr.bank {
                b1 = self.get_ram1(tile_location + u16::from(line));
                b2 = self.get_ram1(tile_location + u16::from(line) + 1);
            } else {
                b1 = self.get_ram0(tile_location + u16::from(line));
                b2 = self.get_ram0(tile_location + u16::from(line) + 1);
            }

            let color_bit = if tile_attr.xflip { px % 8 } else { 7 - px % 8 };
            let color_num =
                if b1 & (1 << color_bit) != 0 { 1 } else { 0 } | if b2 & (1 << color_bit) != 0 { 2 } else { 0 };

            self.bgprio[x] = if color_num == 0 {
                PrioType::Zero
            } else if tile_attr.priority {
                PrioType::Priority
            } else {
                PrioType::Else
            };

            if self.term == Term::GBC {
                let r = self.cbgpal[tile_attr.palette_number_1][color_num][0];
                let g = self.cbgpal[tile_attr.palette_number_1][color_num][1];
                let b = self.cbgpal[tile_attr.palette_number_1][color_num][2];
                self.set_rgb(x as usize, r, g, b);
            } else {
                let color = Self::get_gray_shades(self.bgp, color_num) as u8;
                self.set_gre(x, color);
            }
        }
    }

    fn draw_sprites(&mut self) {
        let sprite_size = if self.lcdc.bit2() { 16 } else { 8 };
        for i in 0..40 {
            let sprite_addr = 0xFE00 + (i as u16) * 4;
            let sprite_y = self.get(sprite_addr + 0) as u16 as i32 - 16;
            let sprite_x = self.get(sprite_addr + 1) as u16 as i32 - 8;
            let tile_location = (self.get(sprite_addr + 2) & (if self.lcdc.bit2() { 0xFE } else { 0xFF })) as u16;
            let tile_attr = Attr::from(self.get(sprite_addr + 3));

            let line = self.ly as i32;
            // If this is true the scanline is out of the area we care about
            if line < sprite_y || line >= sprite_y + sprite_size {
                continue;
            }
            if sprite_x < -7 || sprite_x >= (SCREEN_W as i32) {
                continue;
            }
            let line: u16 = if tile_attr.yflip {
                (sprite_size - 1 - (line - sprite_y)) as u16
            } else {
                (line - sprite_y) as u16
            };
            let tile_location = 0x8000u16 + tile_location * 16 + line * 2;
            let b1: u8;
            let b2: u8;
            if tile_attr.bank && self.term == Term::GBC {
                b1 = self.get_ram1(tile_location);
                b2 = self.get_ram1(tile_location + 1);
            } else {
                b1 = self.get_ram0(tile_location);
                b2 = self.get_ram0(tile_location + 1);
            };

            for x in 0..8 {
                if sprite_x + x < 0 || sprite_x + x >= (SCREEN_W as i32) {
                    continue;
                }
                let color_bit = 1 << (if tile_attr.xflip { x } else { 7 - x } as u32);
                let color_mum = (if b1 & color_bit != 0 { 1 } else { 0 }) | (if b2 & color_bit != 0 { 2 } else { 0 });
                if color_mum == 0 {
                    continue;
                }

                if self.term == Term::GBC {
                    if self.lcdc.bit0()
                        && (self.bgprio[(sprite_x + x) as usize] == PrioType::Priority
                            || (tile_attr.priority && self.bgprio[(sprite_x + x) as usize] != PrioType::Zero))
                    {
                        continue;
                    }
                    let r = self.csprit[tile_attr.palette_number_1][color_mum][0];
                    let g = self.csprit[tile_attr.palette_number_1][color_mum][1];
                    let b = self.csprit[tile_attr.palette_number_1][color_mum][2];
                    self.set_rgb((sprite_x + x) as usize, r, g, b);
                } else {
                    if tile_attr.priority && self.bgprio[(sprite_x + x) as usize] != PrioType::Zero {
                        continue;
                    }
                    let color = if tile_attr.palette_number_0 == 1 {
                        Self::get_gray_shades(self.op1, color_mum) as u8
                    } else {
                        Self::get_gray_shades(self.op0, color_mum) as u8
                    };
                    self.set_gre((sprite_x + x) as usize, color);
                }
            }
        }
    }
}

impl Memory for Gpu {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x8000...0x9fff => self.ram[self.ram_bank][a as usize - 0x8000],
            0xfe00...0xfe9f => self.oam[a as usize - 0xfe00],
            0xff40 => self.lcdc.data,
            0xff41 => {
                let bit6 = if self.stat.enable_ly_interrupt { 0x40 } else { 0x00 };
                let bit5 = if self.stat.enable_m2_interrupt { 0x20 } else { 0x00 };
                let bit4 = if self.stat.enable_m1_interrupt { 0x10 } else { 0x00 };
                let bit3 = if self.stat.enable_m0_interrupt { 0x08 } else { 0x00 };
                let bit2 = if self.ly == self.ly_compare { 0x04 } else { 0x00 };
                bit6 | bit5 | bit4 | bit3 | bit2 | self.stat.mode
            }
            0xff42 => self.sy,
            0xff43 => self.sx,
            0xff44 => self.ly,
            0xff45 => self.ly_compare,
            0xff46 => 0,
            0xff47 => self.bgp,
            0xff48 => self.op0,
            0xff49 => self.op1,
            0xff4a => self.wy,
            0xff4b => self.wx,
            0xff4f => self.ram_bank as u8,
            0xff68 => (if self.cbgpal_inc { 0x80 } else { 0x00 }) | self.cbgpal_ind,
            0xff69 => {
                let r = self.cbgpal_ind as usize >> 3;
                let c = self.cbgpal_ind as usize >> 1 & 0x3;
                if self.cbgpal_ind & 0x01 == 0x00 {
                    let a = self.cbgpal[r][c][0];
                    let b = self.cbgpal[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.cbgpal[r][c][1] >> 3;
                    let b = self.cbgpal[r][c][2] << 2;
                    a | b
                }
            }
            0xff6a => (if self.csprit_inc { 0x80 } else { 0x00 }) | self.csprit_ind,
            0xff6b => {
                let r = self.csprit_ind as usize >> 3;
                let c = self.csprit_ind as usize >> 1 & 0x3;
                if self.csprit_ind & 0x01 == 0x00 {
                    let a = self.csprit[r][c][0];
                    let b = self.csprit[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.csprit[r][c][1] >> 3;
                    let b = self.csprit[r][c][2] << 2;
                    a | b
                }
            }
            _ => panic!("Unsupported address"),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x8000...0x9fff => self.ram[self.ram_bank][a as usize - 0x8000] = v,
            0xfe00...0xfe9f => self.oam[a as usize - 0xfe00] = v,
            0xff40 => {
                self.lcdc.data = v;
                if !self.lcdc.bit7() {
                    self.dots = 0;
                    self.ly = 0;
                    self.stat.mode = 0;
                    // Clean screen.
                    self.data = [[[0xffu8; 3]; SCREEN_W]; SCREEN_H];
                    self.updated = true;
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
            0xff45 => self.ly_compare = v,
            0xff46 => {}
            0xff47 => self.bgp = v,
            0xff48 => self.op0 = v,
            0xff49 => self.op1 = v,
            0xff4a => self.wy = v,
            0xff4b => self.wx = v,
            0xff4f => self.ram_bank = (v & 0x01) as usize,
            0xff68 => {
                self.cbgpal_ind = v & 0x3f;
                self.cbgpal_inc = v & 0x80 != 0x00;
            }
            0xff69 => {
                let r = self.cbgpal_ind as usize >> 3;
                let c = self.cbgpal_ind as usize >> 1 & 0x03;
                if self.cbgpal_ind & 0x01 == 0x00 {
                    self.cbgpal[r][c][0] = v & 0x1f;
                    self.cbgpal[r][c][1] = (self.cbgpal[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.cbgpal[r][c][1] = (self.cbgpal[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.cbgpal[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.cbgpal_inc {
                    self.cbgpal_ind = (self.cbgpal_ind + 1) & 0x3f;
                };
            }
            0xff6a => {
                self.csprit_ind = v & 0x3f;
                self.csprit_inc = v & 0x80 != 0x00;
            }
            0xff6b => {
                let r = self.csprit_ind as usize >> 3;
                let c = self.csprit_ind as usize >> 1 & 0x03;
                if self.csprit_ind & 0x01 == 0x00 {
                    self.csprit[r][c][0] = v & 0x1f;
                    self.csprit[r][c][1] = (self.csprit[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.csprit[r][c][1] = (self.csprit[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.csprit[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.csprit_inc {
                    self.csprit_ind = (self.csprit_ind + 1) & 0x3f;
                };
            }
            _ => panic!("Unsupported address"),
        }
    }
}
