// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use snarkvm::{console::network::Testnet3, prelude::Parser, synthesizer::Program};
type CurrentNetwork = Testnet3;

fn parse(rawp: &str) -> Program<CurrentNetwork> {
    match Program::<CurrentNetwork>::parse(rawp) {
        Ok((remain, program)) => {
            if remain.is_empty() {
                // println!("Good");
                program
            } else {
                panic!("Parser did not consume all of the string: '{remain}'");
            }
        }
        Err(e) => {
            panic!("Parser error: {}", e);
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("No path provided");
    let path = std::path::Path::new(&path);

    // read
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let file = std::fs::File::open(&path).expect("Could not open file");
    let rawp = std::fs::read_to_string(&path).expect("Read error");

    // println!("Read:\n{rawp}");

    let program = parse(&rawp);
    // println!("Program:\n{program}");

    // let j = serde_json::to_string(&program).expect("Json error");
    // println!("Json:\n{j}");

    let jj = program.to_json();
    println!("{jj}\n");
    // println!("Json:\n{jj}");
}
