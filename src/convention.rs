#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Term {
    GB,  // Original GameBoy (GameBoy Classic)
    GBP, // GameBoy Pocket/GameBoy Light
    GBC, // GameBoy Color
    SGB, // Super GameBoy
}
