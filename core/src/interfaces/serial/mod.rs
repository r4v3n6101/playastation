pub mod controller;

pub trait SerialDevice {
    /// Starts a new transfer before receiving its address byte.
    fn begin_transfer(&mut self);

    /// # Returns
    ///
    /// Received byte and ACK for the next byte.
    fn exchange(&mut self, tx: u8) -> (u8, bool);
}
