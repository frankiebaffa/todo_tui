pub struct WindowBufferBounds {
    pub size: u16,
    pub min: u16,
    pub max: u16,
    pub pos: u16,
}
impl WindowBufferBounds {
    // The drawn borders for the Y-bounds take up 1 line apiece
    const Y_PADDING: u16 = 2;
    pub fn init() -> Self {
        Self {
            size: 0,
            min: 0,
            max: 0,
            pos: 0,
        }
    }
    pub fn set_size(&mut self, size: u16, pos: &Vec<usize>, valid_pos: &Vec<Vec<usize>>, initial: bool) -> bool {
        let mut changed = false;
        let new_size = size - Self::Y_PADDING;
        if self.size != new_size {
            self.size = new_size;
            changed = true;
        }
        if self.size > 0 && self.max == 0 {
            self.min = 0;
            self.max = self.size - 1;
            changed = true;
        }
        if initial {
            self.pos = 0;
            changed = true;
        } else {
            // calculate absolute position
            let mut absolute_pos = 0;
            for position in valid_pos.iter() {
                if position == pos {
                    break;
                }
                absolute_pos = absolute_pos + 1;
            }
            if self.pos != absolute_pos {
                self.pos = absolute_pos as u16;
                changed = true;
            }
        }
        // set min/max
        if self.pos >= self.max {
            self.max = self.pos;
            self.min = self.max - (self.size - 1);
            changed = true;
        } else if self.pos < self.min {
            self.min = self.pos;
            self.max = self.min + (self.size - 1);
            changed = true;
        }
        changed
    }
    pub fn is_in_view(&self, line: u16) -> bool {
        line > self.min && line <= (self.max + 1)
    }
}
