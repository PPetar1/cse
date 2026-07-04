//! Player orders: the things a player can tell their units to do on their
//! turn. One module per order type. Shared order plumbing — and the order
//! queue a future WEGO turn system needs — lands here.

mod attack;
mod movement;
