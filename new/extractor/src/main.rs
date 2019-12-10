// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_interface;

use corpus_extractor::analyse;
use rustc_driver::Compilation;
use rustc_interface::{interface::Compiler, Queries};
use std::process;

struct CorpusCallbacks {}

impl rustc_driver::Callbacks for CorpusCallbacks {
    fn after_analysis(&mut self, compiler: &Compiler, queries: &Queries) -> Compilation {
        compiler
            .enter(|queries| analyse(compiler, queries));
        Compilation::Continue
    }
}

fn main() {
    rustc_driver::init_rustc_env_logger();
    let mut callbacks = CorpusCallbacks {};
    let exit_code = rustc_driver::catch_fatal_errors(|| {
        use std::env;
        let mut is_color_arg = false;
        let mut args = env::args()
            .filter(|arg| {
                if arg == "--color" {
                    is_color_arg = true;
                    false
                } else if is_color_arg {
                    is_color_arg = false;
                    false
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();

        args.push("--sysroot".to_owned());
        args.push(std::env::var("SYSROOT").expect("Please specify the SYSROOT env variable."));
        rustc_driver::run_compiler(&args, &mut callbacks, None, None)
    })
    .and_then(|result| result)
    .is_err() as i32;
    process::exit(exit_code);
}
