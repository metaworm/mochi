use anyhow::Result;
use bstr::{ByteVec, B};
use clap::Parser;
use mochi_lua::{
    gc::GcHeap,
    types::{Integer, Table},
    vm::Vm,
};
use rustyline::{error::ReadlineError, Editor};
use std::{ops::Deref, path::PathBuf};

#[cfg(all(feature = "jemalloc", not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[derive(Debug, Parser)]
#[clap(version, about)]
struct Args {
    #[clap(value_parser)]
    script: Option<PathBuf>,

    #[clap(value_parser)]
    args: Vec<String>,

    /// Enter interactive mode after executing <SCRIPT>
    #[clap(value_parser, short, default_value_t = false)]
    interactive: bool,
}

struct LeakingGcHeap(GcHeap);

impl Drop for LeakingGcHeap {
    fn drop(&mut self) {
        // don't bother freeing all the objects when exiting the program
        self.0.leak_all();
    }
}

impl Deref for LeakingGcHeap {
    type Target = GcHeap;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let heap = LeakingGcHeap(GcHeap::new());

    let mut arg = Table::new();
    if let Some(script) = &args.script {
        let script = Vec::from_path_lossy(script);
        arg.set(0, heap.allocate_string(script));
    }
    for (i, x) in args.args.into_iter().enumerate() {
        let key = (i + 1) as Integer;
        let value = heap.allocate_string(x.into_bytes());
        arg.set(key, value);
    }

    let global_table = mochi_lua::create_global_table(&heap);
    global_table
        .borrow_mut(&heap)
        .set_field(heap.allocate_string(B("arg")), heap.allocate_cell(arg));

    let mut vm = Vm::new(&heap, global_table);

    if let Some(script) = args.script {
        let closure = mochi_lua::load_file(&heap, script)?;
        vm.execute(closure)?;

        if !args.interactive {
            return Ok(());
        }
    }

    let mut rl = Editor::<()>::new()?;
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                rl.add_history_entry(&line);
                if let Err(err) = eval(&mut vm, &line) {
                    eprintln!("{}", err);
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => return Ok(()),
            Err(err) => return Err(err.into()),
        }
    }
}

fn eval(vm: &mut Vm, line: &str) -> Result<()> {
    const SOURCE: &str = "stdin";
    let heap = vm.heap();
    let closure = if let Ok(closure) = mochi_lua::load(heap, format!("print({})", line), SOURCE) {
        closure
    } else {
        mochi_lua::load(heap, &line, SOURCE)?
    };
    vm.execute(closure)?;
    Ok(())
}
