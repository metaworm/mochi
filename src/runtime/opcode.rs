macro_rules! impl_from_u8 {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
        $($variant:ident,)*
    }) => {
        $(#[$meta])* $vis enum $name {
            $($variant,)*
        }

        impl From<u8> for $name {
            fn from(i: u8) -> Self {
                const OPCODES: [OpCode; crate::count!($($variant)*)] = [$($name::$variant,)*];
                OPCODES[i as usize]
            }
        }
    }
}

impl_from_u8! {
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    Move,
    LoadI,
    LoadF,
    LoadK,
    LoadKX,
    LoadFalse,
    LFalseSkip,
    LoadTrue,
    LoadNil,
    GetUpval,
    SetUpval,
    GetTabUp,
    GetTable,
    GetI,
    GetField,
    SetTabUp,
    SetTable,
    SetI,
    SetField,
    NewTable,
    Self_,
    AddI,
    AddK,
    SubK,
    MulK,
    ModK,
    PowK,
    DivK,
    IDivK,
    BAndK,
    BOrK,
    BXorK,
    ShrI,
    ShlI,
    Add,
    Sub,
    Mul,
    Mod,
    Pow,
    Div,
    IDiv,
    BAnd,
    BOr,
    BXor,
    Shl,
    Shr,
    MmBin,
    MmBinI,
    MmBinK,
    Unm,
    BNot,
    Not,
    Len,
    Concat,
    Close,
    Tbc,
    Jmp,
    Eq,
    Lt,
    Le,
    EqK,
    EqI,
    LtI,
    LeI,
    GtI,
    GeI,
    Test,
    TestSet,
    Call,
    TailCall,
    Return,
    Return0,
    Return1,
    ForLoop,
    ForPrep,
    TForPrep,
    TForCall,
    TForLoop,
    SetList,
    Closure,
    VarArg,
    VarArgPrep,
    ExtraArg,
}
}

impl std::fmt::Display for OpCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Move => "MOVE",
            Self::LoadI => "LOADI",
            Self::LoadF => "LOADF",
            Self::LoadK => "LOADK",
            Self::LoadKX => "LOADKX",
            Self::LoadFalse => "LOADFALSE",
            Self::LFalseSkip => "LFALSESKIP",
            Self::LoadTrue => "LOADTRUE",
            Self::LoadNil => "LOADNIL",
            Self::GetUpval => "GETUPVAL",
            Self::SetUpval => "SETUPVAL",
            Self::GetTabUp => "GETTABUP",
            Self::GetTable => "GETTABLE",
            Self::GetI => "GETI",
            Self::GetField => "GETFIELD",
            Self::SetTabUp => "SETTABUP",
            Self::SetTable => "SETTABLE",
            Self::SetI => "SETI",
            Self::SetField => "SETFIELD",
            Self::NewTable => "NEWTABLE",
            Self::Self_ => "SELF",
            Self::AddI => "ADDI",
            Self::AddK => "ADDK",
            Self::SubK => "SUBK",
            Self::MulK => "MULK",
            Self::ModK => "MODK",
            Self::PowK => "POWK",
            Self::DivK => "DIVK",
            Self::IDivK => "IDIVK",
            Self::BAndK => "BANDK",
            Self::BOrK => "BORK",
            Self::BXorK => "BXORK",
            Self::ShrI => "SHRI",
            Self::ShlI => "SHLI",
            Self::Add => "ADD",
            Self::Sub => "SUB",
            Self::Mul => "MUL",
            Self::Mod => "MOD",
            Self::Pow => "POW",
            Self::Div => "DIV",
            Self::IDiv => "IDIV",
            Self::BAnd => "BAND",
            Self::BOr => "BOR",
            Self::BXor => "BXOR",
            Self::Shl => "SHL",
            Self::Shr => "SHR",
            Self::MmBin => "MMBIN",
            Self::MmBinI => "MMBINI",
            Self::MmBinK => "MMBINK",
            Self::Unm => "UNM",
            Self::BNot => "BNOT",
            Self::Not => "NOT",
            Self::Len => "LEN",
            Self::Concat => "CONCAT",
            Self::Close => "CLOSE",
            Self::Tbc => "TBC",
            Self::Jmp => "JMP",
            Self::Eq => "EQ",
            Self::Lt => "LT",
            Self::Le => "LE",
            Self::EqK => "EQK",
            Self::EqI => "EQI",
            Self::LtI => "LTI",
            Self::LeI => "LEI",
            Self::GtI => "GTI",
            Self::GeI => "GEI",
            Self::Test => "TEST",
            Self::TestSet => "TESTSET",
            Self::Call => "CALL",
            Self::TailCall => "TAILCALL",
            Self::Return => "RETURN",
            Self::Return0 => "RETURN0",
            Self::Return1 => "RETURN1",
            Self::ForLoop => "FORLOOP",
            Self::ForPrep => "FORPREP",
            Self::TForPrep => "TFORPREP",
            Self::TForCall => "TFORCALL",
            Self::TForLoop => "TFORLOOP",
            Self::SetList => "SETLIST",
            Self::Closure => "CLOSURE",
            Self::VarArg => "VARARG",
            Self::VarArgPrep => "VARARGPREP",
            Self::ExtraArg => "EXTRAARG",
        };
        write!(f, "{:width$}", s, width = f.width().unwrap_or(0))
    }
}
