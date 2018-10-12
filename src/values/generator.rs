// Copyright 2018 The Starlark in Rust Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Generator as a TypedValue
use super::*;
use codemap::CodeMap;
use environment::Environment;
use eval::eval_def;
use std::sync::{Arc, Mutex};
use syntax::ast::AstStatement;

pub struct Generator {
    name: String,
    stmts: AstStatement,
    env: Environment,
    map: Arc<Mutex<CodeMap>>,
}

impl Generator {
    pub fn new<F>(name: String, stmts: AstStatement, map: Arc<Mutex<CodeMap>>) -> Value {
        Value::new(Function {
            function: Box::new(f),
            name: String,
        })
    }
}

/// Define the generator type
impl TypedValue for Generator {
    // TODO: Functions in starlark may not close over non-globals, and globals are immutable by the
    // time a Function is called: therefore, they have no interior mutability. Generators, on the
    // other hand, have internal mutable state and should support being frozen.
    immutable!();
    any!();

    fn to_str(&self) -> String {
        format!("<generator {}>", self.name)
    }
    fn to_repr(&self) -> String {
        self.to_str()
    }

    not_supported!(to_int);
    fn get_type(&self) -> &'static str {
        "generator"
    }
    fn to_bool(&self) -> bool {
        true
    }
    not_supported!(get_hash);

    fn compare(&self, other: &Value, _recursion: u32) -> Result<Ordering, ValueError> {
        if other.get_type() == "generator" {
            Ok(self.to_repr().cmp(&other.to_repr()))
        } else {
            default_compare(self, other)
        }
    }

    fn call(
        &self,
        call_stack: &Vec<(String, String)>,
        env: Environment,
        positional: Vec<Value>,
        named: HashMap<String, Value>,
        args: Option<Value>,
        kwargs: Option<Value>,
    ) -> ValueResult {
        // First map arguments to a vector
        let mut v = Vec::new();
        // Collect args
        let av: Vec<Value> = if let Some(x) = args {
            match x.into_iter() {
                Ok(y) => positional.into_iter().chain(y).collect(),
                Err(..) => return Err(FunctionError::ArgsArrayIsNotIterable.into()),
            }
        } else {
            positional.into_iter().collect()
        };
        let mut args_iter = av.into_iter();
        // Collect kwargs
        let mut kwargs_dict = named.clone();
        if let Some(x) = kwargs {
            match x.into_iter() {
                Ok(y) => {
                    for n in y {
                        if n.get_type() == "string" {
                            let k = n.to_str();
                            if let Ok(v) = x.at(n) {
                                kwargs_dict.insert(k, v);
                            } else {
                                return Err(FunctionError::KWArgsDictIsNotMappable.into());
                            }
                        } else {
                            return Err(FunctionError::ArgsValueIsNotString.into());
                        }
                    }
                }
                Err(..) => return Err(FunctionError::KWArgsDictIsNotMappable.into()),
            }
        }
        // Now verify signature and transform in a value vector
        for parameter in self.signature.iter() {
            match parameter {
                &FunctionParameter::Normal(ref name) => {
                    if let Some(x) = args_iter.next() {
                        v.push(x)
                    } else {
                        if let Some(ref r) = kwargs_dict.remove(name) {
                            v.push(r.clone());
                        } else {
                            return Err(FunctionError::NotEnoughParameter {
                                missing: name.to_string(),
                                function_type: self.function_type.clone(),
                                signature: self.signature.clone(),
                            }
                            .into());
                        }
                    }
                }
                &FunctionParameter::WithDefaultValue(ref name, ref value) => {
                    if let Some(x) = args_iter.next() {
                        v.push(x)
                    } else {
                        if let Some(ref r) = kwargs_dict.remove(name) {
                            v.push(r.clone());
                        } else {
                            v.push(value.clone());
                        }
                    }
                }
                &FunctionParameter::ArgsArray(..) => {
                    let argv: Vec<Value> = args_iter.clone().collect();
                    v.push(Value::from(argv));
                    // We do not use last so we keep ownership of the iterator
                    while args_iter.next().is_some() {}
                }
                &FunctionParameter::KWArgsDict(..) => {
                    v.push(Value::from(kwargs_dict.clone()));
                    kwargs_dict.clear();
                }
            }
        }
        if args_iter.next().is_some() || !kwargs_dict.is_empty() {
            return Err(FunctionError::ExtraParameter.into());
        }
        // Finally call the function with a new child environment
        (*self.function)(
            call_stack,
            env.child(&format!("{}#{}", env.name(), &self.to_str())),
            v,
        )
    }

    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
}
