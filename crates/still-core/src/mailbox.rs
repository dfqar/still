use crate::PointerInput;

#[derive(Clone, Copy, Debug, Default)]
pub struct InputMailbox {
    latest: Option<PointerInput>,
}

impl InputMailbox {
    pub fn push(&mut self, input: PointerInput) {
        self.latest = Some(input);
    }
    pub fn take(&mut self) -> Option<PointerInput> {
        self.latest.take()
    }
}
