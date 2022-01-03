use std::fmt::{Formatter, Display};
use std::slice::from_raw_parts;
use std::mem::size_of;

use crate::bytecode::*;

use colored::*;

use libc::{c_void, malloc, free};

pub enum ExitStatus {
    Success,
    UnknownOpcode,
    BytecodeAccessViolation,
    StackOverflow,
    StackAccessViolation,
    ArithmeticOverflow,
    DivideByZero,
    Unknown,
}

impl Display for ExitStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ExitStatus::Success => "SUCCESS",
            ExitStatus::UnknownOpcode => "UNKNOWN_OPCODE",
            ExitStatus::BytecodeAccessViolation => "BYTECODE_ACCESS_VIOLATION",
            ExitStatus::StackOverflow => "STACK_OVERFLOW",
            ExitStatus::StackAccessViolation => "STACK_ACCESS_VIOLATION",
            ExitStatus::ArithmeticOverflow => "ARITHMETIC_OVERFLOW",
            ExitStatus::DivideByZero => "DIVIDE_BY_ZERO",
            ExitStatus::Unknown => "UNKNOWN",
        };

        return write!(f, "{}", s);
    }
}

impl From<u32> for ExitStatus {
    fn from(v: u32) -> ExitStatus {
        return match v {
            0 => ExitStatus::Success,
            1 => ExitStatus::UnknownOpcode,
            2 => ExitStatus::BytecodeAccessViolation,
            3 => ExitStatus::StackOverflow,
            4 => ExitStatus::StackAccessViolation,
            5 => ExitStatus::ArithmeticOverflow,
            6 => ExitStatus::DivideByZero,
            _ => ExitStatus::Unknown,
        };
    }
}

pub enum Opcode {
    Unknown,
    Nop,
    Exit,
    Invoke,
    Ret,
    BPush,
    SPush,
    IPush,
    LPush,
    Dup,
    Dup2,
    Pop,
    Pop2,
    ILoad,
    LLoad,
    IAdd,
    LAdd,
    ISub,
    LSub,
    IMul,
    LMul,
    IDiv,
    LDiv,
}

impl Display for Opcode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Opcode::Unknown => "unknown",
            Opcode::Nop => "nop",
            Opcode::Exit => "exit",
            Opcode::Invoke => "invoke",
            Opcode::Ret => "ret",
            Opcode::BPush => "bpush",
            Opcode::SPush => "spush",
            Opcode::IPush => "ipush",
            Opcode::LPush => "lpush",
            Opcode::Dup => "dup",
            Opcode::Dup2 => "dup2",
            Opcode::Pop => "pop",
            Opcode::Pop2 => "pop2",
            Opcode::ILoad => "iload",
            Opcode::LLoad => "lload",
            Opcode::IAdd => "iadd",
            Opcode::LAdd => "ladd",
            Opcode::ISub => "isub",
            Opcode::LSub => "lsub",
            Opcode::IMul => "imul",
            Opcode::LMul => "lmul",
            Opcode::IDiv => "idiv",
            Opcode::LDiv => "ldiv",
        };

        return write!(f, "{}", s);
    }
}

impl From<u8> for Opcode {
    fn from(value: u8) -> Opcode {
        return match value {
            0x00 => Opcode::Nop,
            0x01 => Opcode::Exit,
            0x02 => Opcode::Invoke,
            0x03 => Opcode::Ret,
            0x04 => Opcode::BPush,
            0x05 => Opcode::SPush,
            0x06 => Opcode::IPush,
            0x07 => Opcode::LPush,
            0x08 => Opcode::Dup,
            0x09 => Opcode::Dup2,
            0x0a => Opcode::Pop,
            0x0b => Opcode::Pop2,
            0x0c => Opcode::ILoad,
            0x0d => Opcode::LLoad,
            0x0e => Opcode::IAdd,
            0x0f => Opcode::LAdd,
            0x10 => Opcode::ISub,
            0x11 => Opcode::LSub,
            0x12 => Opcode::IMul,
            0x13 => Opcode::LMul,
            0x14 => Opcode::IDiv,
            0x15 => Opcode::LDiv,
            _ => Opcode::Unknown,
        };
    }
}

pub struct Interpreter {}

impl Interpreter {
    pub unsafe fn launch(bytecode_bytes: Vec<u8>) -> ExitStatus {
        let bytecode = Bytecode::new(bytecode_bytes);

        if *HEADER_SIZE > bytecode.len() {
            panic!("{}", "invalid header size".on_red());
        }

        if !bytecode.match_bytes(HeaderItem::MagicNumber.get_bytecode_range(), &MAGIC_NUMBER.to_vec()) {
            panic!("{}", "invalid magic number".on_red());
        }

        bytecode.print();
        return Interpreter::run(&mut *bytecode.into_vec());
    }

    unsafe fn run(bytecode_bytes: &mut Vec<u8>) -> ExitStatus {
        let mut is_init_succeeded = true;
        // note: Exit Status
        let mut es = ExitStatus::Success as u32;

        let bytecode_len = bytecode_bytes.len();
        let bytecode_ptr = bytecode_bytes.as_mut_ptr() as *mut c_void;

        let pool_offset = 128usize;
        let mut pool_ptr = bytecode_ptr.add(pool_offset);

        let entry_point_func_index = *(bytecode_ptr.add(*(pool_ptr as *mut usize)) as *mut usize);
        let entry_point_pc = entry_point_func_index;
        let mut inst_ptr = bytecode_ptr.add(entry_point_pc);

        if entry_point_pc >= bytecode_len {
            is_init_succeeded = false;
            es = ExitStatus::BytecodeAccessViolation as u32;
        }

        let max_stack_size = 1024usize;
        let mut stack_ptr = malloc(max_stack_size) as *mut c_void;

        // note: Stack Pointer
        let mut sp = 0usize;
        // note: Base Pointer
        let mut bp = 0usize;
        // note: Program Counter
        let mut pc = entry_point_pc;
        // note: Pool Pointer
        let mut pp = pool_offset;

        // note: 'operator ブロック外での終了処理
        // fix: 処理が中断されない
        macro_rules! exit {
            ($status_kind:expr) => {
                {
                    es = $status_kind as u32;
                    is_init_succeeded = false;
                }
            };
        }

        macro_rules! jump_to {
            ($ptr:expr, $curr_pos:expr, $jump_to:expr, $size:expr, $err_status:expr) => {
                {
                    if $jump_to > $size {
                        exit!($err_status);
                    }

                    $ptr = $ptr.offset($jump_to as isize - $curr_pos as isize);
                    $curr_pos = $jump_to;
                }
            };
        }

        macro_rules! jump_prg_to {
            ($index:expr) => {
                jump_to!(inst_ptr, pc, $index, bytecode_len, ExitStatus::BytecodeAccessViolation)
            };
        }

        macro_rules! jump_pool_to {
            ($pool_index:expr) => {
                {
                    jump_to!(pool_ptr, pp, pool_offset + $pool_index * size_of::<usize>(), bytecode_len, ExitStatus::BytecodeAccessViolation);
                    let value_addr = next_pool!(usize);
                    jump_to!(pool_ptr, pp, value_addr, bytecode_len, ExitStatus::BytecodeAccessViolation);
                }
            };
        }

        macro_rules! jump_stack_to {
            ($index:expr) => {
                jump_to!(stack_ptr, sp, $index, max_stack_size, ExitStatus::StackAccessViolation)
            };
        }

        macro_rules! push {
            ($ptr:expr, $curr_pos:expr, $ty:ty, $value:expr, $size:expr, $err_status:expr) => {
                {
                    let value_size = size_of::<$ty>();

                    if $curr_pos + value_size > $size {
                        exit!($err_status);
                    }

                    let tmp_ptr = $ptr as *mut $ty;
                    *tmp_ptr = $value;

                    $curr_pos += value_size;
                    $ptr = $ptr.add(value_size);
                }
            };
        }

        macro_rules! stack_push {
            ($ty:ty, $value:expr) => {
                push!(stack_ptr, sp, $ty, $value, max_stack_size, ExitStatus::StackOverflow)
            };

            ($ty:ty, $value:expr, $len:expr) => {
                for _ in 0..$len {
                    stack_push!($ty, $value);
                }
            };
        }

        macro_rules! stack_push_next_prg {
            ($ty:ty $(as $cast_to:ty)?, $push_ty:ty) => {
                {
                    let value = next_prg!($ty) $(as $cast_to)?;
                    stack_push!($push_ty, value);
                }
            };
        }

        macro_rules! stack_dup {
            ($ty:ty) => {
                {
                    let top_value = stack_top!($ty);
                    stack_push!($ty, top_value);
                }
            };
        }

        macro_rules! pop {
            ($ptr:expr, $curr_pos:expr, $ty:ty, $err_status:expr) => {
                {
                    let value_size = size_of::<$ty>();

                    if $curr_pos < value_size {
                        exit!($err_status);
                    }

                    $curr_pos -= value_size;
                    $ptr = $ptr.sub(value_size);

                    *($ptr as *mut $ty)
                }
            };
        }

        macro_rules! stack_pop {
            ($ty:ty) => {
                pop!(stack_ptr, sp, $ty, ExitStatus::StackAccessViolation)
            };

            ($ty:ty, $len:expr) => {
                for _ in 0..$len {
                    stack_pop!($ty);
                }
            };
        }

        macro_rules! load {
            ($ty:ty, $var_index:expr) => {
                {
                    if sp < bp + size_of::<usize>() * 2 {
                        exit!(ExitStatus::StackAccessViolation);
                    }

                    let var_table_top_diff = sp - bp - size_of::<usize>() * 2;

                    if var_table_top_diff < size_of::<u32>() * ($var_index as usize + 1) {
                        exit!(ExitStatus::StackAccessViolation);
                    }

                    let var_table_top = stack_ptr.sub(var_table_top_diff) as *mut u32;
                    let value = *(var_table_top.add($var_index as usize) as *mut $ty);
                    stack_push!($ty, value);
                }
            };
        }

        macro_rules! top {
            ($ptr:expr, $counter:expr, $ty:ty, $err_status:expr) => {
                {
                    let value_size = size_of::<$ty>();

                    if $counter < value_size {
                        exit!($err_status);
                    }

                    *($ptr as *mut $ty).sub(1)
                }
            };
        }

        macro_rules! stack_top {
            ($ty:ty) => {
                top!(stack_ptr, sp, $ty, ExitStatus::StackOverflow)
            };
        }

        macro_rules! next {
            ($ptr:expr, $curr_pos:expr, $ty:ty, $size:expr, $err_status:expr) => {
                {
                    let value_size = size_of::<$ty>();

                    if $curr_pos + value_size > $size {
                        exit!($err_status);
                    }

                    let tmp_ptr = $ptr as *mut $ty;
                    let value = *tmp_ptr;
                    $ptr = (tmp_ptr as *mut c_void).add(value_size);
                    $curr_pos += value_size;

                    value
                }
            };
        }

        macro_rules! next_prg {
            ($ty:ty) => {
                next!(inst_ptr, pc, $ty, bytecode_len, ExitStatus::BytecodeAccessViolation)
            };
        }

        macro_rules! next_pool {
            ($ty:ty) => {
                next!(pool_ptr, pp, $ty, bytecode_len, ExitStatus::BytecodeAccessViolation)
            };
        }

        macro_rules! raw_ptr_to_string {
            ($ptr:expr, $size:expr) => {
                {
                    let mut i = 0usize;
                    let bytes = from_raw_parts($ptr as *const u8, $size).to_vec();

                    if bytes.len() != 0 {
                        bytes.iter().map(|v| {
                            let div = if i != 0 && i % 8 == 0 { "|\n" } else { "" };
                            i += 1;

                            let zero = if format!("{:0x}", v).len() == 1 { "0" } else { "" };

                            format!("{}{}{:0x} ", div, zero, v)
                        }).collect::<Vec<String>>().join("")
                    } else {
                        "<empty>".to_string()
                    }
                }
            };
        }

        macro_rules! calc {
            ($ty:ty, $f:ident$(, $check_divide_by_zero:expr)?) => {
                {
                    let right_term = stack_pop!($ty);
                    let left_term = stack_pop!($ty);

                    $(
                        if $check_divide_by_zero && right_term == 0 {
                            exit!(ExitStatus::DivideByZero);
                        }
                    )?

                    let (value, overflowing) = left_term.$f(right_term);

                    if overflowing {
                        exit!(ExitStatus::ArithmeticOverflow);
                    }

                    stack_push!($ty, value);
                }
            };
        }

        if is_init_succeeded {
            // note: エントリポイント用のコールスタック要素をプッシュ
            // * ベースポインタ
            stack_push!(usize, 0);
            // * リターンアドレス
            stack_push!(usize, bytecode_len - 1);

            'operator: loop {
                // note: 'operator ブロック内での終了処理
                macro_rules! exit {
                    ($status_kind:expr) => {
                        {
                            es = $status_kind as u32;
                            break 'operator;
                        }
                    };
                }

                let tmp_pc = pc;
                let opcode = next_prg!(u8);
                let opcode_kind = Opcode::from(opcode);

                println!("{}", format!("{} (0x{:0x} at 0x{:0x})", opcode_kind.to_string().to_uppercase(), opcode, tmp_pc).blue());
                println!("{}", raw_ptr_to_string!(stack_ptr.sub(sp), sp).bright_black());
                println!();

                match opcode_kind {
                    Opcode::Nop => (),
                    Opcode::Exit => exit!(ExitStatus::Success),
                    Opcode::Invoke => {
                        let pool_i = next_prg!(usize);
                        jump_pool_to!(pool_i);
                        let start_addr = next_pool!(usize);
                        let var_len = next_pool!(u16) as usize;
                        let arg_len = next_pool!(u8) as usize;

                        if var_len < arg_len || sp < arg_len * size_of::<u32>() {
                            exit!(ExitStatus::StackAccessViolation);
                        }

                        // note: 引数値を事前にポップ
                        let mut args = Vec::<u32>::new();

                        for i in 0..arg_len {
                            let new_arg = *((stack_ptr as *mut u32).sub(arg_len - i));
                            args.push(new_arg);
                        }

                        stack_pop!(u32, arg_len);

                        // note: bp をプッシュ & 設定
                        let new_bp = sp;
                        stack_push!(usize, bp);
                        bp = new_bp;

                        // note: リターンアドレスをプッシュ
                        let ret_addr = pc;
                        stack_push!(usize, ret_addr);

                        // note: 引数をプッシュ
                        for each_arg in args {
                            stack_push!(u32, each_arg);
                        }

                        // note: 引数の要素分 (self 参照含む) をスキップ
                        jump_stack_to!(sp + (var_len - arg_len) * size_of::<u32>());

                        // note: 開始アドレスにジャンプ
                        jump_prg_to!(start_addr);

                        println!("{}", format!("[pool index 0x{:0x} / start at 0x{:0x} / return to 0x{:0x} / {} arguments]", pool_i, start_addr, ret_addr, arg_len).bright_green().dimmed());
                        println!();
                    },
                    Opcode::Ret => {
                        if sp < bp || sp - bp < size_of::<usize>() * 2 {
                            exit!(ExitStatus::StackAccessViolation);
                        }

                        // note: オペランドスタックと変数テーブルをポップ
                        let pop_size = sp - bp - size_of::<usize>() * 2;
                        stack_pop!(u8, pop_size);

                        // note: pc 設定
                        let ret_addr = stack_pop!(usize);
                        jump_prg_to!(ret_addr);

                        // note: bp 設定
                        bp = stack_pop!(usize);

                        println!("{}", format!("[return to 0x{:0x} / pop {} bytes / return void]", ret_addr, pop_size).bright_green().dimmed());
                        println!();
                    },
                    Opcode::BPush => stack_push_next_prg!(u8 as u32, u32),
                    Opcode::SPush => stack_push_next_prg!(u16 as u32, u32),
                    Opcode::IPush => stack_push_next_prg!(u32, u32),
                    Opcode::LPush => stack_push_next_prg!(u64, u64),
                    Opcode::Dup => stack_dup!(u32),
                    Opcode::Dup2 => stack_dup!(u64),
                    Opcode::Pop => {
                        let _ = stack_pop!(u32);
                    },
                    Opcode::Pop2 => {
                        let _ = stack_pop!(u64);
                    },
                    Opcode::ILoad => {let a = next_prg!(u16);load!(u32, a)},
                    Opcode::LLoad => load!(u64, next_prg!(u16)),
                    Opcode::IAdd => calc!(u32, overflowing_add),
                    Opcode::LAdd => calc!(u64, overflowing_add),
                    Opcode::ISub => calc!(u32, overflowing_sub),
                    Opcode::LSub => calc!(u64, overflowing_sub),
                    Opcode::IMul => calc!(u32, overflowing_mul),
                    Opcode::LMul => calc!(u64, overflowing_mul),
                    Opcode::IDiv => calc!(u32, overflowing_div, true),
                    Opcode::LDiv => calc!(u64, overflowing_div, true),
                    Opcode::Unknown => exit!(ExitStatus::UnknownOpcode),
                }
            }
        }

        let exit_status_msg = format!("exit status 0x{:0x} ({})", es, ExitStatus::from(es).to_string());

        println!("{}", if es == 0 {
            exit_status_msg.on_bright_black()
        } else {
            exit_status_msg.on_red()
        });

        free(stack_ptr.sub(sp));

        return ExitStatus::from(es);
    }
}
