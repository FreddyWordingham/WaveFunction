use photo::{Direction, ImageRGBA};

/// A candidate `Tile` for a `Map`.
pub struct Tile {
    /// Graphical representation of the `Tile`.
    image: ImageRGBA<u8>,
    /// Relative frequency of the `Tile` in generated `Map`s.
    frequency: usize,
}

impl Tile {
    /// Construct a new `Tile` with a given image and frequency.
    pub fn new(image: ImageRGBA<u8>, frequency: usize) -> Self {
        debug_assert!(image.width() > 0);
        debug_assert!(image.height() > 0);
        debug_assert!(frequency > 0);

        Self { image, frequency }
    }

    /// Get the `Tile`'s image.
    pub fn image(&self) -> &ImageRGBA<u8> {
        &self.image
    }

    /// Get the `Tile`'s frequency.
    pub fn frequency(&self) -> usize {
        self.frequency
    }

    /// Check if this `Tile` is adjacent to another `Tile`.
    pub fn is_adjacent(
        &self,
        other: &Self,
        other_direction: Direction,
        border_size: usize,
    ) -> bool {
        debug_assert!(border_size > 0);

        match other_direction {
            Direction::North | Direction::South => {
                debug_assert!(self.image.height() > border_size);
                debug_assert!(other.image.width() > border_size);
                debug_assert!(self.image.width() == other.image.width());
            }
            Direction::East | Direction::West => {
                debug_assert!(self.image.width() > border_size);
                debug_assert!(other.image.height() > border_size);
                debug_assert!(self.image.height() == other.image.height());
            }
        }

        self.image.view_border(other_direction, border_size)
            == other
                .image
                .view_border(other_direction.opposite(), border_size)
    }
}
