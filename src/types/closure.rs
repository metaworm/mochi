use crate::{
    gc::{GarbageCollect, Gc, GcCell, Tracer},
    types::{LuaString, Value},
    vm::{ErrorKind, Instruction, Vm},
};
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct LuaClosureProto<'gc> {
    pub max_stack_size: u8,
    pub lines_defined: LineRange,
    pub constants: Vec<Value<'gc>>,
    pub code: Vec<Instruction>,
    pub protos: Vec<Gc<'gc, LuaClosureProto<'gc>>>,
    pub upvalues: Vec<UpvalueDescription>,
    pub source: LuaString<'gc>,
}

unsafe impl GarbageCollect for LuaClosureProto<'_> {
    fn trace(&self, tracer: &mut Tracer) {
        self.constants.trace(tracer);
        self.protos.trace(tracer);
        self.source.trace(tracer);
    }
}

#[derive(Debug, Clone)]
pub enum LineRange {
    File,
    Lines(Range<u32>),
}

#[derive(Debug, Clone)]
pub struct LuaClosure<'gc> {
    pub proto: Gc<'gc, LuaClosureProto<'gc>>,
    pub upvalues: Vec<GcCell<'gc, Upvalue<'gc>>>,
}

unsafe impl GarbageCollect for LuaClosure<'_> {
    fn trace(&self, tracer: &mut Tracer) {
        self.proto.trace(tracer);
        self.upvalues.trace(tracer);
    }
}

#[derive(Clone)]
pub struct StackKey(pub(crate) Range<usize>);

pub type NativeClosureFn = dyn Fn(&mut Vm, StackKey) -> Result<usize, ErrorKind>;

pub struct NativeClosure(pub Box<NativeClosureFn>);

impl std::fmt::Debug for NativeClosure {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("NativeClosure").finish()
    }
}

unsafe impl GarbageCollect for NativeClosure {
    fn needs_trace() -> bool {
        false
    }
}

impl NativeClosure {
    pub fn new<T>(func: T) -> Self
    where
        T: 'static + Fn(&mut Vm, StackKey) -> Result<usize, ErrorKind>,
    {
        Self(Box::new(func))
    }
}

#[derive(Debug, Clone)]
pub enum Upvalue<'gc> {
    Open(usize),
    Closed(Value<'gc>),
}

impl<'gc> From<Value<'gc>> for Upvalue<'gc> {
    fn from(x: Value<'gc>) -> Self {
        Self::Closed(x)
    }
}

unsafe impl GarbageCollect for Upvalue<'_> {
    fn trace(&self, tracer: &mut Tracer) {
        if let Self::Closed(x) = self {
            x.trace(tracer);
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpvalueDescription {
    pub in_stack: bool,
    pub index: u8,
}
