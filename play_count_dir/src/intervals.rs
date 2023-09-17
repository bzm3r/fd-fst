#[derive(PartialEq, PartialOrd, Eq, Ord, Copy, Clone, Default, Debug)]
pub struct Start(usize);

#[derive(PartialEq, PartialOrd, Copy, Clone, Default, Debug)]
pub struct End(Option<usize>);

impl End {
    /// Checks if this end point is less than the given start point.
    ///
    /// If this end point is unspecified (`None`), then it is not less than
    /// any start point.
    #[inline]
    pub fn lt_start(&self, start: &Start) -> bool {
        self.0
            .map(|finite_end| finite_end <= start.0)
            .unwrap_or(false)
    }
}

pub enum Interval {
    Empty,
    NonEmpty(NonEmptyInterval),
}

pub struct NonEmptyInterval {
    start: Start,
    end: End,
}

impl NonEmptyInterval {
    #[inline]
    fn disjoint_assuming_left(&self, right: &NonEmptyInterval) -> bool {
        #[cfg(debug_assertions)]
        assert!(self.start < right.start);

        if self.start == right.start {
            false
        } else {
            right.end.lt_start(&self.start)
        }
    }
}

impl Interval {
    #[inline]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            _ => false,
        }
    }

    #[inline]
    fn is_disjoint(&self, other: &Interval) -> bool {
        match (self, other) {
            (Self::Empty, _) => false,
            (_, Self::Empty) => false,
            (Self::NonEmpty(this), Self::NonEmpty(other)) => Self::non_empty_disjoint(this, other),
        }
    }
}

#[inline]
fn disjoint_assuming_left_right(left: &NonEmptyInterval, right: &NonEmptyInterval) -> bool {}

#[inline]
fn non_empty_disjoint(this: &NonEmptyInterval, other: &NonEmptyInterval) -> bool {
    if this.start < other.start {
        this.disjoint_assuming_left(other)
    } else {
        other.disjoint_assuming_left(this)
    }
}
