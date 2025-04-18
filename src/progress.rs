/// A trait representing a progress sink like a progress bar.
pub trait ProgressReceiverBuilder {
    type Initialized: ProgressReceiver;

    /// Create a progress receiver with the total number
    fn init(self, total: u64) -> Self::Initialized;
}

/// A trait representing a progress sink that has been initialized.
pub trait ProgressReceiver {
    /// Set the progress to the given position.
    fn set_position(&self, position: u64);

    /// Finish the progress
    fn finish(&self);
}
