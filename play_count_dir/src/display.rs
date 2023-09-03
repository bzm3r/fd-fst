use std::time::Duration;

use crate::{
    adp_num::{AbsoluteNum, AdaptorNum},
    num_absf64::AbsF64,
    num_hist::{HistData, HistoryNum},
    sig_figs::RoundSigFigs,
};

pub trait CustomDisplay {
    fn custom_display(&self) -> String;
}

impl CustomDisplay for String {
    fn custom_display(&self) -> String {
        self.to_string()
    }
}

impl CustomDisplay for f64 {
    fn custom_display(&self) -> String {
        format!("{:e}", self.round_sig_figs(4))
    }
}

impl CustomDisplay for &f64 {
    fn custom_display(&self) -> String {
        format!("{:e}", self.round_sig_figs(4))
    }
}

impl CustomDisplay for Duration {
    fn custom_display(&self) -> String {
        format!("{}s", self.as_secs_f64().custom_display())
    }
}

impl CustomDisplay for &Duration {
    fn custom_display(&self) -> String {
        format!("{}s", self.as_secs_f64().custom_display())
    }
}

impl CustomDisplay for usize {
    fn custom_display(&self) -> String {
        format!("{self}")
    }
}

impl CustomDisplay for &usize {
    fn custom_display(&self) -> String {
        format!("{self}")
    }
}

impl CustomDisplay for AbsF64 {
    fn custom_display(&self) -> String {
        self.inner().custom_display()
    }
}

impl CustomDisplay for &AbsF64 {
    fn custom_display(&self) -> String {
        self.inner().custom_display()
    }
}

impl CustomDisplay for isize {
    fn custom_display(&self) -> String {
        format!("{self}")
    }
}

impl CustomDisplay for &isize {
    fn custom_display(&self) -> String {
        format!("{self}")
    }
}

impl<Absolute, Adaptor> CustomDisplay for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor> + CustomDisplay,
    Adaptor: AdaptorNum<Absolute>,
{
    fn custom_display(&self) -> String {
        self.absolute().custom_display()
    }
}

impl<T> CustomDisplay for Option<T>
where
    T: CustomDisplay,
{
    fn custom_display(&self) -> String {
        if let Some(inner) = self {
            inner.custom_display()
        } else {
            "None".to_string()
        }
    }
}
