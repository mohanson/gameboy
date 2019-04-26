// General Memory Map
// 0000-3FFF   16KB ROM Bank 00     (in cartridge, fixed at bank 00)
// 4000-7FFF   16KB ROM Bank 01..NN (in cartridge, switchable bank number)
// 8000-9FFF   8KB Video RAM (VRAM) (switchable bank 0-1 in CGB Mode)
// A000-BFFF   8KB External RAM     (in cartridge, switchable bank, if any)
// C000-CFFF   4KB Work RAM Bank 0 (WRAM)
// D000-DFFF   4KB Work RAM Bank 1 (WRAM)  (switchable bank 1-7 in CGB Mode)
// E000-FDFF   Same as C000-DDFF (ECHO)    (typically not used)
// FE00-FE9F   Sprite Attribute Table (OAM)
// FEA0-FEFF   Not Usable
// FF00-FF7F   I/O Ports
// FF80-FFFE   High RAM (HRAM)
// FFFF        Interrupt Enable Register
//
// See: http://bgb.bircd.org/pandocs.htm#cgbregisters
pub trait Memory {
    fn get(&self, a: u16) -> u8;

    fn set(&mut self, a: u16, v: u8);

    fn get_word(&self, a: u16) -> u16 {
        u16::from(self.get(a)) | (u16::from(self.get(a + 1)) << 8)
    }

    fn set_word(&mut self, a: u16, v: u16) {
        self.set(a, (v & 0xFF) as u8);
        self.set(a + 1, (v >> 8) as u8)
    }
}
