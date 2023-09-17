use std::time::Duration;

use crate::{
    adp_num::{AbsoluteNum, AdaptorNum},
    num_absf64::AbsF64,
    num_hist::{HistData, HistoryNum},
    sig_figs::RoundSigFigs,
};

pub trait NumDisplay {
    fn num_display(&self) -> String;
}

impl NumDisplay for f64 {
    fn num_display(&self) -> String {
        format!("{:e}", self.round_sig_figs(4))
    }
}

impl NumDisplay for &f64 {
    fn num_display(&self) -> String {
        format!("{:e}", self.round_sig_figs(4))
    }
}

impl NumDisplay for Duration {
    fn num_display(&self) -> String {
        format!("{}s", self.as_secs_f64().num_display())
    }
}

impl NumDisplay for &Duration {
    fn num_display(&self) -> String {
        format!("{}s", self.as_secs_f64().num_display())
    }
}

impl NumDisplay for usize {
    fn num_display(&self) -> String {
        format!("{self}")
    }
}

impl NumDisplay for &usize {
    fn num_display(&self) -> String {
        format!("{self}")
    }
}

impl NumDisplay for u8 {
    fn num_display(&self) -> String {
        format!("{self}")
    }
}

impl NumDisplay for &u8 {
    fn num_display(&self) -> String {
        format!("{self}")
    }
}

impl NumDisplay for AbsF64 {
    fn num_display(&self) -> String {
        self.inner().num_display()
    }
}

impl NumDisplay for &AbsF64 {
    fn num_display(&self) -> String {
        self.inner().num_display()
    }
}

impl NumDisplay for isize {
    fn num_display(&self) -> String {
        format!("{self}")
    }
}

impl NumDisplay for &isize {
    fn num_display(&self) -> String {
        format!("{self}")
    }
}

impl<Absolute, Adaptor> NumDisplay for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor> + NumDisplay,
    Adaptor: AdaptorNum<Absolute>,
{
    fn num_display(&self) -> String {
        self.absolute().num_display()
    }
}

impl<T> NumDisplay for Option<T>
where
    T: NumDisplay,
{
    fn num_display(&self) -> String {
        if let Some(inner) = self {
            inner.num_display()
        } else {
            "None".to_string()
        }
    }
}
