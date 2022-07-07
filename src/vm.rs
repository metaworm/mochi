mod error;
mod instruction;
mod opcode;
mod ops;

pub use error::{ErrorKind, Operation, RuntimeError, TracebackFrame};
pub use instruction::Instruction;
pub use opcode::OpCode;

use crate::{
    gc::{GcCell, GcHeap, Trace, Tracer},
    types::{Integer, LuaString, Number, StackKey, Table, Upvalue, Value},
    LuaClosure,
};
use std::{
    cmp::PartialOrd,
    collections::BTreeMap,
    ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Shl, Shr, Sub},
};

#[derive(Debug, Clone)]
struct Frame {
    bottom: usize,
    pc: usize,
}

#[derive(Debug)]
pub(crate) struct State<'a, 'b> {
    bottom: usize,
    pc: usize,
    stack: &'b mut [Value<'a>],
    lower_stack: &'b [Value<'a>],
}

unsafe impl Trace for State<'_, '_> {
    fn trace(&self, tracer: &mut Tracer) {
        self.stack.trace(tracer);
        self.lower_stack.trace(tracer);
    }
}

impl<'a, 'b> State<'a, 'b> {
    fn resolve_upvalue(&'b self, upvalue: &'b Upvalue<'a>) -> &'b Value<'a> {
        match upvalue {
            Upvalue::Open(i) => {
                let boundary = self.bottom + 1;
                if *i < boundary {
                    &self.lower_stack[*i]
                } else {
                    &self.stack[*i - boundary]
                }
            }
            Upvalue::Closed(x) => x,
        }
    }
}

#[derive(Debug)]
pub struct Vm<'a> {
    stack: Vec<Value<'a>>,
    frames: Vec<Frame>,
    global_table: GcCell<'a, Table<'a>>,
    open_upvalues: BTreeMap<usize, GcCell<'a, Upvalue<'a>>>,
}

unsafe impl Trace for Vm<'_> {
    fn trace(&self, tracer: &mut Tracer) {
        self.stack.trace(tracer);
        self.global_table.trace(tracer);
        self.open_upvalues.trace(tracer);
    }
}

impl<'a> Vm<'a> {
    pub fn new(global_table: GcCell<'a, Table<'a>>) -> Self {
        Self {
            stack: Vec::new(),
            frames: Vec::new(),
            global_table,
            open_upvalues: BTreeMap::new(),
        }
    }

    pub fn global_table(&self) -> GcCell<'a, Table<'a>> {
        self.global_table
    }

    pub fn local_stack(&self, key: StackKey) -> &[Value<'a>] {
        &self.stack[key.0]
    }

    pub fn local_stack_mut(&mut self, key: StackKey) -> &mut [Value<'a>] {
        &mut self.stack[key.0]
    }

    pub fn execute(
        &mut self,
        heap: &'a GcHeap,
        mut closure: LuaClosure<'a>,
    ) -> Result<Value<'a>, RuntimeError> {
        assert!(closure.upvalues.is_empty());
        closure
            .upvalues
            .push(heap.allocate_cell(Value::from(self.global_table).into()));

        let bottom = self.stack.len();
        self.stack.push(heap.allocate(closure).into());

        let frame_level = self.frames.len();
        self.frames.push(Frame { bottom, pc: 0 });

        while self.frames.len() > frame_level {
            if let Err(source) = self.execute_frame(heap) {
                let traceback = self
                    .frames
                    .iter()
                    .rev()
                    .map(|frame| {
                        let value = &self.stack[frame.bottom];
                        let proto = &value.as_lua_closure().unwrap().proto;
                        TracebackFrame {
                            source: String::from_utf8_lossy(&proto.source).to_string(),
                            lines_defined: proto.lines_defined.clone(),
                        }
                    })
                    .collect();
                return Err(RuntimeError { source, traceback });
            }
        }

        let result = self.stack.drain(bottom..).next().unwrap_or_default();
        unsafe { heap.step(self) };
        Ok(result)
    }

    fn execute_frame(&mut self, heap: &'a GcHeap) -> Result<(), ErrorKind> {
        let frame = self.frames.last().unwrap().clone();
        let prev_stack_top = self.stack.len();
        self.stack.resize(
            {
                let value = &self.stack[frame.bottom];
                let closure = value.as_lua_closure().unwrap();
                frame.bottom + 1 + closure.proto.max_stack_size as usize
            },
            Value::Nil,
        );

        let (lower_stack, stack) = self.stack.split_at_mut(frame.bottom + 1);
        let bottom_value = lower_stack.last().unwrap();
        let closure = bottom_value.as_lua_closure().unwrap();
        let mut state = State {
            bottom: frame.bottom,
            pc: frame.pc,
            stack,
            lower_stack,
        };

        loop {
            let insn = closure.proto.code[state.pc];
            let opcode = insn.opcode();
            state.pc += 1;

            match opcode {
                OpCode::Move => state.stack[insn.a()] = state.stack[insn.b()],
                OpCode::LoadI => state.stack[insn.a()] = Value::Integer(insn.sbx() as Integer),
                OpCode::LoadF => state.stack[insn.a()] = Value::Number(insn.sbx() as Number),
                OpCode::LoadK => {
                    state.stack[insn.a()] = closure.proto.constants[insn.bx()];
                }
                OpCode::LoadKX => {
                    let next_insn = closure.proto.code[state.pc];
                    let rb = state.stack[next_insn.ax()];
                    state.stack[insn.a()] = rb;
                    state.pc += 1;
                }
                OpCode::LoadFalse => state.stack[insn.a()] = Value::Boolean(false),
                OpCode::LFalseSkip => {
                    state.stack[insn.a()] = Value::Boolean(false);
                    state.pc += 1;
                }
                OpCode::LoadTrue => state.stack[insn.a()] = Value::Boolean(true),
                OpCode::LoadNil => state.stack[insn.a()..][..insn.b()].fill(Value::Nil),
                OpCode::GetUpval => {
                    let upvalue = closure.upvalues[insn.b()].borrow();
                    let value = state.resolve_upvalue(&upvalue);
                    state.stack[insn.a()] = *value;
                }
                OpCode::SetUpval => unimplemented!("SETUPVAL"),
                OpCode::GetTabUp => {
                    state.stack[insn.a()] = {
                        let upvalue = closure.upvalues[insn.b()].borrow();
                        let value = state.resolve_upvalue(&upvalue);
                        let table = value.as_table().ok_or_else(|| ErrorKind::TypeError {
                            operation: Operation::Index,
                            ty: value.ty(),
                        })?;
                        let rc = closure.proto.constants[insn.c() as usize];
                        table.get(rc)
                    };
                }
                OpCode::GetTable => {
                    state.stack[insn.a()] = {
                        let rb = &state.stack[insn.b()];
                        let rc = state.stack[insn.c() as usize];
                        let table = rb.as_table().ok_or_else(|| ErrorKind::TypeError {
                            operation: Operation::Index,
                            ty: rb.ty(),
                        })?;
                        table.get(rc)
                    };
                }
                OpCode::GetI => {
                    state.stack[insn.a()] = {
                        let rb = &state.stack[insn.b()];
                        let table = rb.as_table().ok_or_else(|| ErrorKind::TypeError {
                            operation: Operation::Index,
                            ty: rb.ty(),
                        })?;
                        let c = insn.c() as Integer;
                        table.get(c)
                    };
                }
                OpCode::GetField => {
                    state.stack[insn.a()] = {
                        let rb = &state.stack[insn.b()];
                        let table = rb.as_table().ok_or_else(|| ErrorKind::TypeError {
                            operation: Operation::Index,
                            ty: rb.ty(),
                        })?;
                        let rc = closure.proto.constants[insn.c() as usize];
                        table.get(rc)
                    };
                }
                OpCode::SetTabUp => {
                    let kb = closure.proto.constants[insn.b()];
                    let upvalue = closure.upvalues[insn.a()].borrow();
                    let table_value = state.resolve_upvalue(&upvalue);
                    let mut table =
                        table_value
                            .as_table_mut(heap)
                            .ok_or_else(|| ErrorKind::TypeError {
                                operation: Operation::Index,
                                ty: table_value.ty(),
                            })?;
                    let c = insn.c() as usize;
                    let rkc = if insn.k() {
                        closure.proto.constants[c]
                    } else {
                        state.stack[c]
                    };
                    table.set(kb, rkc);
                }
                OpCode::SetTable => {
                    let ra = &state.stack[insn.a()];
                    let mut table = ra.as_table_mut(heap).ok_or_else(|| ErrorKind::TypeError {
                        operation: Operation::Index,
                        ty: ra.ty(),
                    })?;
                    let rb = state.stack[insn.b()];
                    let c = insn.c() as usize;
                    let rkc = if insn.k() {
                        closure.proto.constants[c]
                    } else {
                        state.stack[c]
                    };
                    table.set(rb, rkc);
                }
                OpCode::SetI => {
                    let ra = &state.stack[insn.a()];
                    let mut table = ra.as_table_mut(heap).ok_or_else(|| ErrorKind::TypeError {
                        operation: Operation::Index,
                        ty: ra.ty(),
                    })?;
                    let b = insn.b() as Integer;
                    let c = insn.c() as usize;
                    let rkc = if insn.k() {
                        closure.proto.constants[c]
                    } else {
                        state.stack[c]
                    };
                    table.set(b, rkc);
                }
                OpCode::SetField => {
                    let ra = state.stack[insn.a()];
                    let mut table = ra.as_table_mut(heap).ok_or_else(|| ErrorKind::TypeError {
                        operation: Operation::Index,
                        ty: ra.ty(),
                    })?;
                    let kb = closure.proto.constants[insn.b()];
                    let c = insn.c() as usize;
                    let rkc = if insn.k() {
                        closure.proto.constants[c]
                    } else {
                        state.stack[c]
                    };
                    table.set(kb, rkc);
                }
                OpCode::NewTable => {
                    state.stack[insn.a()] = heap.allocate_cell(Table::new()).into();
                    state.pc += 1;
                }
                OpCode::Self_ => {
                    let a = insn.a();
                    let rb = state.stack[insn.b()];
                    state.stack[a + 1] = rb;
                    let table = rb.as_table().ok_or_else(|| ErrorKind::TypeError {
                        operation: Operation::Index,
                        ty: rb.ty(),
                    })?;
                    let c = insn.c() as usize;
                    let rkc = if insn.k() {
                        closure.proto.constants[c]
                    } else {
                        state.stack[c]
                    };
                    state.stack[a] = table.get(rkc);
                }
                OpCode::AddI => {
                    ops::do_arithmetic_with_immediate(&mut state, insn, Integer::add, Number::add)
                }
                OpCode::AddK => ops::do_arithmetic_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::add,
                    Number::add,
                ),
                OpCode::SubK => ops::do_arithmetic_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::sub,
                    Number::sub,
                ),
                OpCode::MulK => ops::do_arithmetic_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::mul,
                    Number::mul,
                ),
                OpCode::ModK => unimplemented!("MODK"),
                OpCode::PowK => ops::do_float_arithmetic_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Number::powf,
                ),
                OpCode::DivK => ops::do_float_arithmetic_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Number::div,
                ),
                OpCode::IDivK => unimplemented!("IDIVK"),
                OpCode::BAndK => ops::do_bitwise_op_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::bitand,
                ),
                OpCode::BOrK => ops::do_bitwise_op_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::bitor,
                ),
                OpCode::BXorK => ops::do_bitwise_op_with_constant(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::bitxor,
                ),
                OpCode::ShrI => unimplemented!("SHRI"),
                OpCode::ShlI => unimplemented!("SHLI"),
                OpCode::Add => ops::do_arithmetic(&mut state, insn, Integer::add, Number::add),
                OpCode::Sub => ops::do_arithmetic(&mut state, insn, Integer::sub, Number::sub),
                OpCode::Mul => ops::do_arithmetic(&mut state, insn, Integer::mul, Number::mul),
                OpCode::Mod => unimplemented!("MOD"),
                OpCode::Pow => ops::do_float_arithmetic(&mut state, insn, Number::powf),
                OpCode::Div => ops::do_float_arithmetic(&mut state, insn, Number::div),
                OpCode::IDiv => unimplemented!("IDIV"),
                OpCode::BAnd => ops::do_bitwise_op(&mut state, insn, Integer::bitand),
                OpCode::BOr => ops::do_bitwise_op(&mut state, insn, Integer::bitor),
                OpCode::BXor => ops::do_bitwise_op(&mut state, insn, Integer::bitxor),
                OpCode::Shr => ops::do_bitwise_op(&mut state, insn, Integer::shr),
                OpCode::Shl => ops::do_bitwise_op(&mut state, insn, Integer::shl),
                OpCode::MmBin => unimplemented!("MMBIN"),
                OpCode::MmBinI => unimplemented!("MMBINI"),
                OpCode::MmBinK => unimplemented!("MMBINK"),
                OpCode::Unm => {
                    state.stack[insn.a()] = {
                        let rb = &state.stack[insn.b()];
                        if let Some(x) = rb.as_integer() {
                            Value::Integer(-x)
                        } else if let Some(x) = rb.as_number() {
                            Value::Number(-x)
                        } else {
                            unimplemented!("UNM")
                        }
                    };
                }
                OpCode::BNot => {
                    state.stack[insn.a()] = {
                        let rb = &state.stack[insn.b()];
                        if let Some(x) = rb.as_integer() {
                            Value::Integer(!x)
                        } else {
                            unimplemented!("BNOT")
                        }
                    }
                }
                OpCode::Not => {
                    state.stack[insn.a()] = {
                        let rb = &state.stack[insn.b()];
                        Value::Boolean(!rb.as_boolean())
                    }
                }
                OpCode::Len => unimplemented!("LEN"),
                OpCode::Concat => {
                    let a = insn.a();
                    let b = insn.b();
                    let mut strings = Vec::with_capacity(b);
                    for value in state.stack[a..].iter().take(b) {
                        if let Some(string) = value.as_lua_string() {
                            strings.push(string);
                        } else {
                            return Err(ErrorKind::TypeError {
                                operation: Operation::Concatenate,
                                ty: value.ty(),
                            });
                        }
                    }
                    let strings: Vec<_> = strings.iter().map(|x| x.as_bytes()).collect();
                    state.stack[a] = heap.allocate(LuaString::from(strings.concat())).into();
                }
                OpCode::Close => unimplemented!("CLOSE"),
                OpCode::Tbc => unimplemented!("TBC"),
                OpCode::Jmp => state.pc = (state.pc as isize + insn.sj() as isize) as usize,
                OpCode::Eq => ops::do_comparison(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::eq,
                    Number::eq,
                    PartialEq::eq,
                ),
                OpCode::Lt => ops::do_comparison(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::lt,
                    Number::lt,
                    PartialOrd::lt,
                ),
                OpCode::Le => ops::do_comparison(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::le,
                    Number::le,
                    PartialOrd::le,
                ),
                OpCode::EqK => {
                    let ra = state.stack[insn.a()];
                    let rb = closure.proto.constants[insn.b()];
                    let cond = ra == rb;
                    ops::do_conditional_jump(&mut state, &closure.proto, insn, cond)
                }
                OpCode::EqI => ops::do_comparison_with_immediate(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::eq,
                    Number::eq,
                ),
                OpCode::LtI => ops::do_comparison_with_immediate(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::lt,
                    Number::lt,
                ),
                OpCode::LeI => ops::do_comparison_with_immediate(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::le,
                    Number::le,
                ),
                OpCode::GtI => ops::do_comparison_with_immediate(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::gt,
                    Number::gt,
                ),
                OpCode::GeI => ops::do_comparison_with_immediate(
                    &mut state,
                    &closure.proto,
                    insn,
                    Integer::ge,
                    Number::ge,
                ),
                OpCode::Test => {
                    let cond = state.stack[insn.a()].as_boolean();
                    ops::do_conditional_jump(&mut state, &closure.proto, insn, cond);
                }
                OpCode::TestSet => unimplemented!("TESTSET"),
                OpCode::Call => {
                    self.frames.last_mut().unwrap().pc = state.pc;
                    let a = insn.a();
                    let ra = state.stack[a];
                    match &ra {
                        Value::LuaClosure(_) => {
                            self.frames.push(Frame {
                                bottom: frame.bottom + a + 1,
                                pc: 0,
                            });
                            return Ok(());
                        }
                        Value::NativeClosure(closure) => {
                            let b = insn.b();
                            let start = frame.bottom + 1 + a;
                            let range = if b > 0 {
                                start..start + b // fixed number of args
                            } else {
                                start..prev_stack_top // variable number of args
                            };
                            let num_results = (closure.0)(heap, self, StackKey(range))?;
                            self.stack.truncate(start + num_results);
                            return Ok(());
                        }
                        value => {
                            return Err(ErrorKind::TypeError {
                                operation: Operation::Call,
                                ty: value.ty(),
                            });
                        }
                    }
                }
                OpCode::TailCall => unimplemented!("TAILCALL"),
                OpCode::Return => {
                    if insn.k() {
                        for (_, upvalue) in self.open_upvalues.split_off(&frame.bottom) {
                            let mut upvalue = upvalue.borrow_mut(heap);
                            if let Upvalue::Open(i) = *upvalue {
                                *upvalue = Upvalue::Closed(self.stack[i]);
                            }
                        }
                    }
                    let a = insn.a();
                    let b = insn.b();
                    let num_results = if b > 0 { b - 1 } else { self.stack.len() - a };
                    for i in 0..num_results {
                        self.stack[frame.bottom + i] = self.stack[frame.bottom + 1 + a];
                    }
                    self.stack.truncate(frame.bottom + num_results);
                    self.frames.pop().unwrap();
                    return Ok(());
                }
                OpCode::Return0 => {
                    self.stack.truncate(frame.bottom);
                    self.frames.pop().unwrap();
                    return Ok(());
                }
                OpCode::Return1 => {
                    self.stack[frame.bottom] = state.stack[insn.a()];
                    self.stack.truncate(frame.bottom + 1);
                    self.frames.pop().unwrap();
                    return Ok(());
                }
                OpCode::ForLoop => {
                    let a = insn.a();
                    if let Some(step) = state.stack[a + 2].as_integer() {
                        let count = state.stack[a + 1].as_integer().unwrap();
                        if count > 0 {
                            let index = state.stack[a].as_integer().unwrap();
                            state.stack[a + 1] = (count - 1).into();
                            let index = Value::from(index + step);
                            state.stack[a] = index;
                            state.stack[a + 3] = index;
                            state.pc -= insn.bx();
                        }
                    } else {
                        unimplemented!("FORLOOP")
                    }
                }
                OpCode::ForPrep => {
                    let a = insn.a();
                    if let (Some(init), Some(limit), Some(step)) = (
                        state.stack[a].as_integer(),
                        state.stack[a + 1].as_integer(),
                        state.stack[a + 2].as_integer(),
                    ) {
                        assert!(step != 0);
                        let skip = if step > 0 { init > limit } else { init < limit };
                        if skip {
                            state.pc += insn.bx() + 1;
                        } else {
                            state.stack[a + 3] = state.stack[a];
                            let count = if step > 0 {
                                (limit - init) / step
                            } else {
                                (init - limit) / (-(step + 1) + 1)
                            };
                            state.stack[a + 1] = count.into();
                        }
                    } else {
                        unimplemented!("FORPREP")
                    }
                }
                OpCode::TForPrep => unimplemented!("TFORPREP"),
                OpCode::TForCall => unimplemented!("TFORCALL"),
                OpCode::TForLoop => unimplemented!("TFORLOOP"),
                OpCode::SetList => {
                    let a = insn.a();
                    let b = insn.b();
                    let c = insn.c() as usize;
                    let ra = &state.stack[a];
                    let mut table = ra.as_table_mut(heap).ok_or_else(|| ErrorKind::TypeError {
                        operation: Operation::Index,
                        ty: ra.ty(),
                    })?;
                    for (i, x) in state.stack[a + 1..=a + b].iter().cloned().enumerate() {
                        table.set((c + i + 1) as Integer, x);
                    }
                }
                OpCode::Closure => {
                    let proto = closure.proto.protos[insn.bx()];
                    let upvalues = proto
                        .upvalues
                        .iter()
                        .map(|desc| {
                            if desc.in_stack {
                                let index = frame.bottom + 1 + desc.index as usize;
                                *self
                                    .open_upvalues
                                    .entry(index)
                                    .or_insert_with(|| heap.allocate_cell(Upvalue::Open(index)))
                            } else {
                                closure.upvalues[desc.index as usize]
                            }
                        })
                        .collect();
                    state.stack[insn.a()] = heap.allocate(LuaClosure { proto, upvalues }).into();
                }
                OpCode::VarArg => unimplemented!("VARARG"),
                OpCode::VarArgPrep => (),
                OpCode::ExtraArg => unreachable!(),
            }

            struct Root<'a, 'b> {
                state: &'b State<'a, 'b>,
                global_table: &'b GcCell<'a, Table<'a>>,
                open_upvalues: &'b BTreeMap<usize, GcCell<'a, Upvalue<'a>>>,
            }
            unsafe impl Trace for Root<'_, '_> {
                fn trace(&self, tracer: &mut Tracer) {
                    self.state.trace(tracer);
                    self.global_table.trace(tracer);
                    self.open_upvalues.trace(tracer);
                }
            }
            let root = Root {
                state: &state,
                global_table: &self.global_table,
                open_upvalues: &self.open_upvalues,
            };
            unsafe { heap.step(&root) };
        }
    }
}
