use crate::convention::Memory;

// Communication between two Gameboys happens one byte at a time. One Gameboy acts as the master, uses its internal
// clock, and thus controls when the exchange happens. The other one uses an external clock (i.e., the one inside the
// other Gameboy) and has no control over when the transfer happens. If it hasn't gotten around to loading up the next
// data byte at the time the transfer begins, the last one will go out again. Alternately, if it's ready to send the
// next byte but the last one hasn't gone out yet, it has no choice but to wait.
//
// See: http://gbdev.gg8.se/wiki/articles/Serial_Data_Transfer_(Link_Cable)
pub struct Serial {
    // Before a transfer, it holds the next byte that will go out.
    // During a transfer, it has a blend of the outgoing and incoming bytes. Each cycle, the leftmost bit is shifted
    // out (and over the wire) and the incoming bit is shifted in from the other side:
    pub data: u8,
    // Bit 7 - Transfer Start Flag (0=No transfer is in progress or requested, 1=Transfer in progress, or requested)
    // Bit 1 - Clock Speed (0=Normal, 1=Fast) ** CGB Mode Only **
    // Bit 0 - Shift Clock (0=External Clock, 1=Internal Clock)
    pub control: u8,
}

impl Serial {
    pub fn power_up() -> Self {
        Self { data: 0x00, control: 0x00 }
    }
}

impl Memory for Serial {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff01 => self.data,
            0xff02 => self.control,
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff01 => self.data = v,
            0xff02 => self.control = v,
            _ => unreachable!(),
        };
    }
}
