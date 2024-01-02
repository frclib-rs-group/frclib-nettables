#[derive(Debug)]
pub(super) enum UnsignedIntOrNegativeOne {
    NegativeOne,
    UnsignedInt(u32),
}

impl UnsignedIntOrNegativeOne {
    pub(crate) fn write_to_buf<W: rmp::encode::RmpWrite>(
        &self,
        wr: &mut W,
    ) -> Result<(), rmp::encode::ValueWriteError<W::Error>> {
        match self {
            Self::NegativeOne => rmp::encode::write_i32(wr, -1),
            Self::UnsignedInt(u_int) => rmp::encode::write_u32(wr, *u_int),
        }
    }
}
