use crate::convention::{Global, Memory, Term, Ticker};
use crate::interrupt::{Interrupt, InterruptFlag};
use std::cell::RefCell;
use std::rc::Rc;

// LCDC is the main LCD Control register. Its bits toggle what elements are displayed on the screen, and how.
pub struct Lcdc {
    data: u8,
}

#[rustfmt::skip]
impl Lcdc {
    pub fn power_up() -> Self {
        Self { data: 0x48 }
    }

    // LCDC.7 - LCD Display Enable
    // This bit controls whether the LCD is on and the PPU is active. Setting it to 0 turns both off, which grants
    // immediate and full access to VRAM, OAM, etc.
    pub fn bit7(&self) -> bool { self.data & 0b1000_0000 != 0x00 }

    // LCDC.6 - Window Tile Map Display Select
    // This bit controls which background map the Window uses for rendering. When it's reset, the $9800 tilemap is used,
    // otherwise it's the $9C00 one.
    pub fn bit6(&self) -> bool { self.data & 0b0100_0000 != 0x00 }

    // LCDC.5 - Window Display Enable
    // This bit controls whether the window shall be displayed or not. (TODO : what happens when toggling this
    // mid-scanline ?) This bit is overridden on DMG by bit 0 if that bit is reset.
    // Note that on CGB models, setting this bit to 0 then back to 1 mid-frame may cause the second write to be ignored.
    pub fn bit5(&self) -> bool { self.data & 0b0010_0000 != 0x00 }

    // LCDC.4 - BG & Window Tile Data Select
    // This bit controls which addressing mode the BG and Window use to pick tiles.
    // Sprites aren't affected by this, and will always use $8000 addressing mode.
    pub fn bit4(&self) -> bool { self.data & 0b0001_0000 != 0x00 }

    // LCDC.3 - BG Tile Map Display Select
    // This bit works similarly to bit 6: if the bit is reset, the BG uses tilemap $9800, otherwise tilemap $9C00.
    pub fn bit3(&self) -> bool { self.data & 0b0000_1000 != 0x00 }

    // LCDC.2 - OBJ Size
    // This bit controls the sprite size (1 tile or 2 stacked vertically).
    // Be cautious when changing this mid-frame from 8x8 to 8x16 : "remnants" of the sprites intended for 8x8 could
    // "leak" into the 8x16 zone and cause artifacts.
    pub fn bit2(&self) -> bool { self.data & 0b0000_0100 != 0x00 }

    // LCDC.1 - OBJ Display Enable
    // This bit toggles whether sprites are displayed or not.
    // This can be toggled mid-frame, for example to avoid sprites being displayed on top of a status bar or text box.
    // (Note: toggling mid-scanline might have funky results on DMG? Investigation needed.)
    pub fn bit1(&self) -> bool { self.data & 0b0000_0010 != 0x00 }


    // LCDC.0 - BG/Window Display/Priority
    // LCDC.0 has different meanings depending on Gameboy type and Mode:
    // Monochrome Gameboy, SGB and CGB in Non-CGB Mode: BG Display
    // When Bit 0 is cleared, both background and window become blank (white), and the Window Display Bit is ignored in
    // that case. Only Sprites may still be displayed (if enabled in Bit 1).
    // CGB in CGB Mode: BG and Window Master Priority
    // When Bit 0 is cleared, the background and window lose their priority - the sprites will be always displayed on
    // top of background and window, independently of the priority flags in OAM and BG Map attributes.
    pub fn bit0(&self) -> bool { self.data & 0b0000_0001 != 0x00 }
}

// LCD Status Register.
pub struct Stat {
    data: u8,
}

#[rustfmt::skip]
impl Stat {
    pub fn power_up() -> Self {
        Self { data: 0x00 }
    }

    // Stat.6 - LYC=LY Coincidence Interrupt (1=Enable) (Read/Write)
    pub fn bit6(&self) -> bool { self.data & 0b0100_0000 != 0x00 }

    // Stat.5 - Mode 2 OAM Interrupt         (1=Enable) (Read/Write)
    pub fn bit5(&self) -> bool { self.data & 0b0010_0000 != 0x00 }

    // Stat.4 - Mode 1 V-Blank Interrupt     (1=Enable) (Read/Write)
    pub fn bit4(&self) -> bool { self.data & 0b0001_0000 != 0x00 }

    // Stat.3 - Mode 0 H-Blank Interrupt     (1=Enable) (Read/Write)
    pub fn bit3(&self) -> bool { self.data & 0b0000_1000 != 0x00 }

    // Stat.2 - LYC=LY Coincidence Flag (0=Different, 1=Equal) (Read Only)
    pub fn bit2(&self) -> bool { self.data & 0b0000_0100 != 0x00 }

    // Set or clear the LYC=LY coincidence flag (Stat.2).
    pub fn lyeq(&mut self, v: bool) { if v { self.data |= 0b0000_0100 } else { self.data &= !0b0000_0100 } }

    // Stat.0 - PPU Mode (0=HBlank, 1=VBlank, 2=OAM Scan, 3=Pixel Transfer) (Read Only)
    pub fn mode(&self) -> u8 { self.data & 0x03 }
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
pub struct Bgpi {
    data: u8,
}

impl Bgpi {
    pub fn power_up() -> Self {
        Self { data: 0x00 }
    }

    // Bgpi.5-0 - Palette Memory Address (00-3F)
    pub fn addr(&self) -> u8 {
        self.data & 0x3f
    }

    // Bgpi.7 - Auto Increment (0=Disabled, 1=Increment after Writing)
    pub fn auto(&self) -> bool {
        self.data & 0b1000_0000 != 0x00
    }

    // When the auto increment bit is set then the index is automatically incremented after each write to FF69.
    pub fn incr(&mut self) {
        self.data = (self.data & 0x80) | self.addr().wrapping_add(1) & 0x3f;
    }
}

impl Memory for Bgpi {
    fn lb(&self, _: u16) -> u8 {
        self.data
    }

    fn sb(&mut self, _: u16, v: u8) {
        self.data = v & 0xbf;
    }
}

pub enum GrayShades {
    White = 0xff,
    Light = 0xc0,
    Dusky = 0x60,
    Black = 0x00,
}

impl From<u8> for GrayShades {
    fn from(u: u8) -> Self {
        match u {
            0x00 => GrayShades::White,
            0x01 => GrayShades::Light,
            0x02 => GrayShades::Dusky,
            0x03 => GrayShades::Black,
            _ => unreachable!(),
        }
    }
}

// See: https://gbdev.io/pandocs/OAM.html#byte-3--attributesflags
struct Attr {
    data: u8,
}

#[rustfmt::skip]
impl Attr {
    // Attr.7 - OBJ-to-BG Priority (0=OBJ Above BG, 1=OBJ Behind BG color 1-3)
    fn bit7(&self) -> bool { self.data & 0b1000_0000 != 0x00 }

    // Attr.6 - Y Flip (0=Normal, 1=Vertically mirrored)
    fn bit6(&self) -> bool { self.data & 0b0100_0000 != 0x00 }

    // Attr.5 - X Flip (0=Normal, 1=Horizontally mirrored)
    fn bit5(&self) -> bool { self.data & 0b0010_0000 != 0x00 }

    // Attr.4 - DMG Palette [Non-CGB only] (0=OBP0, 1=OBP1)
    fn bit4(&self) -> bool { self.data & 0b0001_0000 != 0x00 }

    // Attr.3 - Tile VRAM Bank [CGB only] (0=Bank 0, 1=Bank 1)
    fn bit3(&self) -> bool { self.data & 0b0000_1000 != 0x00 }

    // Attr.2-0 - CGB Palette Number [CGB only] (OBP0-7)
    fn paln(&self) -> usize { (self.data & 0x07) as usize }
}

impl From<u8> for Attr {
    fn from(u: u8) -> Self {
        Self { data: u }
    }
}

pub const SCREEN_W: usize = 160;
pub const SCREEN_H: usize = 144;

pub struct Gpu {
    glo: Rc<RefCell<Global>>,

    // Digital image with mode RGB. Size = 144 * 160 * 3.
    // 3---------
    // ----------
    // ----------
    // ---------- 160
    //        144
    pub data: [[[u8; 3]; SCREEN_W]; SCREEN_H],
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
    // Extra dots to add to the mode3 threshold due to sprites on the current scanline.
    // Computed when mode2 fires; represents floor(T_penalty/4)*4 extra dots before mode0.
    sprite_penalty: u32,
    // Set when LCDC bit 7 transitions 0->1. Line 0 after LCD enable starts in Mode 0
    // (not Mode 2) and jumps directly to Mode 3 at dot 80.
    lcdon_first_line: bool,
    // Tracks the STAT IRQ signal level (the OR of all enabled STAT interrupt sources).
    // The LCD interrupt (IF bit 1) is only raised on a 0→1 RISING EDGE of this signal.
    // This implements the "STAT IRQ blocking" behaviour: if one source (e.g. LYC=LY)
    // keeps the signal HIGH through mode 3, the mode-0 transition does NOT generate
    // a second interrupt because the signal never goes LOW in between.
    stat_irq: bool,
}

impl Gpu {
    pub fn power_up(glo: Rc<RefCell<Global>>) -> Self {
        Self {
            glo,
            data: [[[0xffu8; 3]; SCREEN_W]; SCREEN_H],
            h_blank: false,
            v_blank: false,
            lcdc: Lcdc::power_up(),
            stat: Stat::power_up(),
            sy: 0x00,
            sx: 0x00,
            wy: 0x00,
            wx: 0x00,
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
            sprite_penalty: 0,
            lcdon_first_line: false,
            stat_irq: false,
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
    // 1  Light
    // 2  Dusky
    // 3  Black
    fn get_gray_shades(v: u8, i: usize) -> GrayShades {
        GrayShades::from(v >> (2 * i) & 0x03)
    }

    // Compute the STAT IRQ signal level: HIGH if any enabled interrupt source is active.
    // The signal is LOW during mode 3 (no STAT interrupt source) unless LYC=LY keeps it high.
    // When LCD is disabled the signal is always LOW.
    fn stat_irq_level(&self) -> bool {
        if !self.lcdc.bit7() {
            return false;
        }
        (self.stat.bit3() && self.stat.mode() == 0)
            || (self.stat.bit4() && self.stat.mode() == 1)
            || (self.stat.bit5() && self.stat.mode() == 2)
            || (self.stat.bit6() && self.stat.bit2())
    }

    // Recompute the STAT IRQ signal and raise an LCD interrupt on a 0→1 rising edge.
    // Must be called after any change that could affect the signal:
    //   mode transitions, stat.bit2() changes, STAT enable-bit writes, LCDC writes.
    // When LCD is OFF this is a no-op: stat_irq is "frozen" at the level it had when
    // the LCD was last on.  This ensures that re-enabling the LCD only fires an interrupt
    // if the comparison result actually changes (0→1), not just because the off-state
    // appeared as level=0 and any match looks like a rising edge.
    fn stat_irq_update(&mut self) {
        if !self.lcdc.bit7() {
            // Freeze: don't touch stat_irq while LCD is off.
            return;
        }
        let new_level = self.stat_irq_level();
        if new_level && !self.stat_irq {
            Interrupt::owned(self.glo.clone()).raise(InterruptFlag::LCD);
        }
        self.stat_irq = new_level;
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

    pub fn check_and_reset_gpu_updated(&mut self) -> bool {
        let result = self.v_blank;
        self.v_blank = false;
        result
    }

    /// Apply the DMG OAM write-corruption bug to the currently scanned OAM row.
    ///
    /// According to Pan Docs: OAM is split into 20 rows of 8 bytes each; during mode 2
    /// the PPU reads one row per M-cycle (every 4 T-cycles).  When an IDU write is
    /// triggered (INC/DEC rr with rr in $FE00–$FEFF), the currently accessed row is
    /// corrupted as follows (treating the 8-byte row as four 16-bit little-endian words):
    ///
    ///   • First word  ← `((a ^ c) & (b ^ c)) ^ c`
    ///   • Last three words ← last three words of the *preceding* row
    ///
    /// where a = first word of current row, b = first word of preceding row,
    /// c = third word of preceding row.
    ///
    /// The first row (row 0, objects 0–1) is immune.
    pub fn oam_write_corrupt(&mut self) {
        if !self.lcdc.bit7() || self.stat.mode() != 2 || self.ly >= 144 {
            return;
        }
        let row = (self.dots / 4) as usize;
        if row == 0 || row >= 20 {
            // Row 0 is immune; row >= 20 is outside mode-2 range.
            return;
        }
        let cur_start = row * 8;
        let prev_start = (row - 1) * 8;

        // Read a, b, c as little-endian 16-bit words.
        let a = (self.oam[cur_start] as u16) | ((self.oam[cur_start + 1] as u16) << 8);
        let b = (self.oam[prev_start] as u16) | ((self.oam[prev_start + 1] as u16) << 8);
        let c = (self.oam[prev_start + 4] as u16) | ((self.oam[prev_start + 5] as u16) << 8);

        let new_first = ((a ^ c) & (b ^ c)) ^ c;
        self.oam[cur_start] = new_first as u8;
        self.oam[cur_start + 1] = (new_first >> 8) as u8;
        // Last three words (bytes 2–7) copied from preceding row.
        for i in 2..8 {
            self.oam[cur_start + i] = self.oam[prev_start + i];
        }
    }

    /// Apply the DMG OAM read-corruption bug to the currently scanned OAM row.
    ///
    /// Triggered when a CPU memory *read* lands in $FE00–$FEFF during mode 2 (e.g.
    /// `ld a, (hl)` with HL in OAM; or the read half of `ld a, [hli]`).
    ///
    ///   • First word  ← `b | (a & c)`
    ///   • Last three words ← last three words of the preceding row
    ///
    /// Row 0 is immune (same as write corruption).
    pub fn oam_read_corrupt(&mut self) {
        if !self.lcdc.bit7() || self.stat.mode() != 2 || self.ly >= 144 {
            return;
        }
        let row = (self.dots / 4) as usize;
        if row == 0 || row >= 20 {
            return;
        }
        let cur_start = row * 8;
        let prev_start = (row - 1) * 8;

        let a = (self.oam[cur_start] as u16) | ((self.oam[cur_start + 1] as u16) << 8);
        let b = (self.oam[prev_start] as u16) | ((self.oam[prev_start + 1] as u16) << 8);
        let c = (self.oam[prev_start + 4] as u16) | ((self.oam[prev_start + 5] as u16) << 8);

        let new_first = b | (a & c);
        self.oam[cur_start] = new_first as u8;
        self.oam[cur_start + 1] = (new_first >> 8) as u8;
        for i in 2..8 {
            self.oam[cur_start + i] = self.oam[prev_start + i];
        }
    }

    /// Apply the DMG OAM "Read During Increase/Decrease" corruption pattern.
    ///
    /// Triggered when a CPU memory *read* from OAM happens in the same M-cycle as
    /// an IDU operation (e.g. POP M2: bus_read[sp] + sp++).
    ///
    /// For rows 4–18 (inclusive):
    ///   1. The first word of the *preceding* row is replaced with:
    ///      `(b & (a | c | d)) | (a & c & d)`
    ///      where a = first word two rows before, b = first word of preceding row,
    ///      c = first word of current row, d = third word of preceding row.
    ///   2. The preceding row (with corrupted first word) is copied to both the
    ///      current row and two rows before.
    ///
    /// A normal read corruption is then applied to the current row regardless.
    pub fn oam_rdi_corrupt(&mut self) {
        if !self.lcdc.bit7() || self.stat.mode() != 2 || self.ly >= 144 {
            return;
        }
        let row_cur = (self.dots / 4) as usize;
        if row_cur == 0 || row_cur >= 20 {
            return;
        }

        // Complex pattern only for rows 4–18.
        if row_cur >= 4 && row_cur < 19 {
            let row_pre = row_cur - 1;
            let row_pre2 = row_cur - 2;

            let a = (self.oam[row_pre2 * 8] as u16) | ((self.oam[row_pre2 * 8 + 1] as u16) << 8);
            let b = (self.oam[row_pre * 8] as u16) | ((self.oam[row_pre * 8 + 1] as u16) << 8);
            let c = (self.oam[row_cur * 8] as u16) | ((self.oam[row_cur * 8 + 1] as u16) << 8);
            let d = (self.oam[row_pre * 8 + 4] as u16) | ((self.oam[row_pre * 8 + 5] as u16) << 8);

            let new_b = (b & (a | c | d)) | (a & c & d);
            self.oam[row_pre * 8] = new_b as u8;
            self.oam[row_pre * 8 + 1] = (new_b >> 8) as u8;

            // Copy preceding row (with corrupted first word) to current and two-rows-before.
            for i in 0..8 {
                self.oam[row_pre2 * 8 + i] = self.oam[row_pre * 8 + i];
                self.oam[row_cur * 8 + i] = self.oam[row_pre * 8 + i];
            }
        }

        // Always apply read corruption to the current row.
        let cur_start = row_cur * 8;
        let prev_start = (row_cur - 1) * 8;
        let a = (self.oam[cur_start] as u16) | ((self.oam[cur_start + 1] as u16) << 8);
        let b = (self.oam[prev_start] as u16) | ((self.oam[prev_start + 1] as u16) << 8);
        let c = (self.oam[prev_start + 4] as u16) | ((self.oam[prev_start + 5] as u16) << 8);
        let new_first = b | (a & c);
        self.oam[cur_start] = new_first as u8;
        self.oam[cur_start + 1] = (new_first >> 8) as u8;
        for i in 2..8 {
            self.oam[cur_start + i] = self.oam[prev_start + i];
        }
    }

    // Compute the sprite timing penalty (in T-cycles) for the current scanline.
    // Each sprite on the scanline with OAM_X < 168 adds:
    //   - 6T (tile fetch cost)
    //   - max(0, 5 - min(5, (oam_x + scx) % 8)) T (alignment cost, once per unique X value)
    // Only the first 10 sprites per scanline are counted (DMG hardware limit).
    // The effective extra dots = (T_penalty / 4) * 4 (rounded down to 4T boundary).
    fn compute_sprite_penalty(&self) -> u32 {
        if !self.lcdc.bit1() {
            return 0;
        }
        let sprite_size: i32 = if self.lcdc.bit2() { 16 } else { 8 };
        let ly = self.ly as i32;
        let mut penalty = 0u32;
        let mut seen_x = [false; 256];
        let mut count = 0u32;

        for i in 0..40usize {
            let oam_y = self.oam[i * 4] as i32;
            let oam_x = self.oam[i * 4 + 1];

            let sprite_top = oam_y - 16;
            let sprite_bottom = sprite_top + sprite_size - 1;
            if ly < sprite_top || ly > sprite_bottom {
                continue;
            }

            // Sprites at OAM_X >= 168 are off-screen right and don't affect mode3 timing
            if oam_x >= 168 {
                continue;
            }

            count += 1;
            if count > 10 {
                break;
            }

            penalty += 6;

            if !seen_x[oam_x as usize] {
                seen_x[oam_x as usize] = true;
                let fine = ((oam_x as u32) + (self.sx as u32)) % 8;
                if fine < 5 {
                    penalty += 5 - fine;
                }
            }
        }
        penalty
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

            let tile_y = if tile_attr.bit6() { 7 - py % 8 } else { py % 8 };
            let tile_y_data: [u8; 2] = if self.glo.borrow().term == Term::CGB && tile_attr.bit3() {
                let a = self.get_ram1(tile_location + u16::from(tile_y * 2));
                let b = self.get_ram1(tile_location + u16::from(tile_y * 2) + 1);
                [a, b]
            } else {
                let a = self.get_ram0(tile_location + u16::from(tile_y * 2));
                let b = self.get_ram0(tile_location + u16::from(tile_y * 2) + 1);
                [a, b]
            };
            let tile_x = if tile_attr.bit5() { 7 - px % 8 } else { px % 8 };

            // Palettes
            let color_l = if tile_y_data[0] & (0x80 >> tile_x) != 0 { 1 } else { 0 };
            let color_h = if tile_y_data[1] & (0x80 >> tile_x) != 0 { 2 } else { 0 };
            let color = color_h | color_l;

            // Priority
            self.prio[x] = (tile_attr.bit7(), color);

            if self.glo.borrow().term == Term::CGB {
                let r = self.cbgpd[tile_attr.paln()][color][0];
                let g = self.cbgpd[tile_attr.paln()][color][1];
                let b = self.cbgpd[tile_attr.paln()][color][2];
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
            let py = self.lb(sprite_addr).wrapping_sub(16);
            let px = self.lb(sprite_addr + 1).wrapping_sub(8);
            let tile_number = self.lb(sprite_addr + 2) & if self.lcdc.bit2() { 0xfe } else { 0xff };
            let tile_attr = Attr::from(self.lb(sprite_addr + 3));

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
                if tile_attr.bit6() { sprite_size - 1 - self.ly.wrapping_sub(py) } else { self.ly.wrapping_sub(py) };
            let tile_y_addr = 0x8000u16 + u16::from(tile_number) * 16 + u16::from(tile_y) * 2;
            let tile_y_data: [u8; 2] = if self.glo.borrow().term == Term::CGB && tile_attr.bit3() {
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
                let tile_x = if tile_attr.bit5() { 7 - x } else { x };

                // Palettes
                let color_l = if tile_y_data[0] & (0x80 >> tile_x) != 0 { 1 } else { 0 };
                let color_h = if tile_y_data[1] & (0x80 >> tile_x) != 0 { 2 } else { 0 };
                let color = color_h | color_l;
                if color == 0 {
                    continue;
                }

                // Confirm the priority of background and sprite.
                let prio = self.prio[px.wrapping_add(x) as usize];
                let skip = if self.glo.borrow().term == Term::CGB && !self.lcdc.bit0() {
                    prio.1 == 0
                } else if prio.0 {
                    prio.1 != 0
                } else {
                    tile_attr.bit7() && prio.1 != 0
                };
                if skip {
                    continue;
                }

                if self.glo.borrow().term == Term::CGB {
                    let r = self.cobpd[tile_attr.paln()][color][0];
                    let g = self.cobpd[tile_attr.paln()][color][1];
                    let b = self.cobpd[tile_attr.paln()][color][2];
                    self.set_rgb(px.wrapping_add(x) as usize, r, g, b);
                } else {
                    let color = if tile_attr.bit4() {
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
    fn lb(&self, a: u16) -> u8 {
        match a {
            0x8000..=0x9fff => {
                // VRAM is locked during mode 3, and also during the 4T pre-mode-3 period
                // (mode 2, dots >= 76) where the bus is already claimed by the GPU,
                // matching real DMG hardware bus timing.
                let vram_locked = self.stat.mode() == 3 || (self.stat.mode() == 2 && self.dots >= 76);
                if vram_locked { 0xff } else { self.ram[self.ram_bank * 0x2000 + a as usize - 0x8000] }
            }
            0xfe00..=0xfe9f => {
                // OAM is locked (returns 0xFF) during mode 2 (OAM scan) and mode 3 (pixel transfer).
                // It is also locked from dot 452 onward (LY increment / OAM scan preparation),
                // even though STAT still reports mode 0 at that dot.
                let oam_locked =
                    self.stat.mode() == 2 || self.stat.mode() == 3 || (self.stat.mode() == 0 && self.dots >= 452);
                if oam_locked { 0xff } else { self.oam[a as usize - 0xfe00] }
            }
            0xff40 => self.lcdc.data,
            0xff41 => {
                let bit2 = if self.stat.bit2() { 0x04 } else { 0x00 };
                // Bit 7 is unused and always reads as 1
                0x80 | (self.stat.data & 0x78) | bit2 | self.stat.mode()
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
            0xff68 => self.cbgpi.lb(0xff68),
            0xff69 => {
                let r = self.cbgpi.addr() as usize >> 3;
                let c = self.cbgpi.addr() as usize >> 1 & 0x3;
                if self.cbgpi.addr() & 0x01 == 0x00 {
                    let a = self.cbgpd[r][c][0];
                    let b = self.cbgpd[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.cbgpd[r][c][1] >> 3;
                    let b = self.cbgpd[r][c][2] << 2;
                    a | b
                }
            }
            0xff6a => self.cobpi.lb(0xff6a),
            0xff6b => {
                let r = self.cobpi.addr() as usize >> 3;
                let c = self.cobpi.addr() as usize >> 1 & 0x3;
                if self.cobpi.addr() & 0x01 == 0x00 {
                    let a = self.cobpd[r][c][0];
                    let b = self.cobpd[r][c][1] << 5;
                    a | b
                } else {
                    let a = self.cobpd[r][c][1] >> 3;
                    let b = self.cobpd[r][c][2] << 2;
                    a | b
                }
            }
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0x8000..=0x9fff => {
                // VRAM writes are ignored during mode 3 (pixel transfer).
                if self.stat.mode() != 3 {
                    self.ram[self.ram_bank * 0x2000 + a as usize - 0x8000] = v;
                }
            }
            0xfe00..=0xfe9f => {
                // OAM writes are ignored during mode 3 (pixel transfer) and during mode 2
                // (OAM scan) while dots < 76. The last 4T of mode 2 (dots 76-79) the OAM
                // scan is already complete and the CPU can write to OAM again.
                let oam_write_blocked = self.stat.mode() == 3 || (self.stat.mode() == 2 && self.dots < 76);
                if !oam_write_blocked {
                    self.oam[a as usize - 0xfe00] = v;
                }
            }
            0xff40 => {
                let was_on = self.lcdc.bit7();
                self.lcdc.data = v;
                if !self.lcdc.bit7() {
                    self.dots = 0;
                    self.ly = 0;
                    self.stat.data &= !0x03;
                    // Clean screen.
                    self.data = [[[0xffu8; 3]; SCREEN_W]; SCREEN_H];
                    self.v_blank = true;
                    // LCD disabled: signal goes LOW unconditionally.
                    self.stat_irq_update();
                } else if !was_on {
                    // LCD just enabled: line 0 starts in Mode 0, not Mode 2.
                    // Evaluate LYC=LY coincidence for the initial display state.
                    self.lcdon_first_line = true;
                    self.stat.lyeq(self.ly == self.lc);
                    self.stat_irq_update();
                }
            }
            0xff41 => {
                // Bits 3-6 are writable; bits 0-1 (mode) are read-only (PPU-controlled).
                self.stat.data = (self.stat.data & 0x07) | (v & 0x78);
                // Enabling a source that is currently active generates a rising edge.
                self.stat_irq_update();
            }
            0xff42 => self.sy = v,
            0xff43 => self.sx = v,
            0xff44 => {}
            0xff45 => {
                self.lc = v;
                // The comparison clock only runs while the LCD is on.
                // Writing LYC while LCD is off must not change stat.bit2()
                // (STAT bit 2 is frozen during LCD-off).
                if self.lcdc.bit7() {
                    self.stat.lyeq(self.ly == self.lc);
                    self.stat_irq_update();
                }
            }
            0xff47 => self.bgp = v,
            0xff48 => self.op0 = v,
            0xff49 => self.op1 = v,
            0xff4a => self.wy = v,
            0xff4b => self.wx = v,
            0xff4f => self.ram_bank = (v & 0x01) as usize,
            0xff68 => self.cbgpi.sb(0xff68, v),
            0xff69 => {
                let r = self.cbgpi.addr() as usize >> 3;
                let c = self.cbgpi.addr() as usize >> 1 & 0x03;
                if self.cbgpi.addr() & 0x01 == 0x00 {
                    self.cbgpd[r][c][0] = v & 0x1f;
                    self.cbgpd[r][c][1] = (self.cbgpd[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.cbgpd[r][c][1] = (self.cbgpd[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.cbgpd[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.cbgpi.auto() {
                    self.cbgpi.incr();
                }
            }
            0xff6a => self.cobpi.sb(0xff6a, v),
            0xff6b => {
                let r = self.cobpi.addr() as usize >> 3;
                let c = self.cobpi.addr() as usize >> 1 & 0x03;
                if self.cobpi.addr() & 0x01 == 0x00 {
                    self.cobpd[r][c][0] = v & 0x1f;
                    self.cobpd[r][c][1] = (self.cobpd[r][c][1] & 0x18) | (v >> 5);
                } else {
                    self.cobpd[r][c][1] = (self.cobpd[r][c][1] & 0x07) | ((v & 0x03) << 3);
                    self.cobpd[r][c][2] = (v >> 2) & 0x1f;
                }
                if self.cobpi.auto() {
                    self.cobpi.incr();
                }
            }
            _ => unreachable!(),
        }
    }
}

impl Ticker for Gpu {
    fn tick(&mut self, cycles: u16) {
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
        // When LCD is disabled the PPU is completely halted; dots and LY freeze.
        if !self.lcdc.bit7() {
            return;
        }
        if cycles == 0 {
            return;
        }
        let c = (cycles - 1) / 80 + 1;
        for i in 0..c {
            if i == (c - 1) {
                self.dots += cycles as u32 % 80
            } else {
                self.dots += 80
            }
            let d = self.dots;
            self.dots %= 456;
            if d == 452 {
                // LY increments 4T before the end of the scanline.
                // The STAT LYC=LY bit is cleared immediately: it will be re-evaluated
                // when the new scanline starts (dots=0), matching DMG hardware behaviour.
                self.ly = (self.ly + 1) % 154;
                self.stat.lyeq(false);
                // Update the STAT IRQ signal — the LYC=LY source just went inactive.
                // The new LY's LYC coincidence is re-evaluated at mode-2 start (dots=0).
                self.stat_irq_update();
                // Skip mode transitions; they fire on the next tick when dots=0
                continue;
            }
            if self.ly >= 144 {
                if self.stat.mode() == 1 {
                    continue;
                }
                self.stat.data = (self.stat.data & !0x03) | 1;
                self.stat.lyeq(self.ly == self.lc);
                self.v_blank = true;
                Interrupt::owned(self.glo.clone()).raise(InterruptFlag::VBlank);
                // DMG quirk: at line 144, a Mode 2 OAM pulse is generated simultaneously
                // with Mode 1 (VBlank) entry.  If the M2 interrupt is enabled and the
                // STAT signal was LOW, fire a rising edge now (before stat_irq_update
                // sets the persistent level based on Mode 1 / LYC).
                if self.stat.bit5() && !self.stat_irq {
                    Interrupt::owned(self.glo.clone()).raise(InterruptFlag::LCD);
                }
                self.stat_irq_update();
            } else if self.dots < 80 {
                if self.lcdon_first_line {
                    // Line 0 after LCD enable: stay in Mode 0, skip Mode 2 entirely.
                    continue;
                }
                if self.stat.mode() == 2 {
                    continue;
                }
                self.stat.data = (self.stat.data & !0x03) | 2;
                self.stat.lyeq(self.ly == self.lc);
                // Compute sprite timing penalty for this scanline's mode3 window
                let t_pen = self.compute_sprite_penalty();
                self.sprite_penalty = (t_pen / 4) * 4;
                self.stat_irq_update();
            } else if self.dots <= (80 + 172 + ((self.sx as u32 % 8 + 3) / 4) * 4) - 4 + self.sprite_penalty {
                self.lcdon_first_line = false;
                self.stat.data = (self.stat.data & !0x03) | 3;
                // Mode 3 has no STAT interrupt source; the signal may fall to LOW here
                // unless LYC=LY keeps it high.  Update to track any falling edge.
                self.stat_irq_update();
            } else {
                if self.stat.mode() == 0 {
                    continue;
                }
                self.stat.data &= !0x03;
                self.h_blank = true;
                self.stat_irq_update();
                // Render scanline
                if self.glo.borrow().term == Term::CGB || self.lcdc.bit0() {
                    self.draw_bg();
                }
                if self.lcdc.bit1() {
                    self.draw_sprites();
                }
            }
        }
    }
}
