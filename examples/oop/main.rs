#![allow(dead_code, unused_macros, unreachable_code, unused_variables)]
enum GrfExit<T> {
    Fall,
    Return(T),
    Break(&'static str),
    Continue(&'static str),
    ContinueInner(&'static str),
    Error(graphite_core::diagnostic::RuntimeError),
}

macro_rules! grf_add {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_add($b) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_sub {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_sub($b) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_mul {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_mul($b) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_div {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_div($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    return Err(graphite_core::diagnostic::division_by_zero($site));
                } else {
                    return Err(graphite_core::diagnostic::integer_overflow($site));
                }
            }
        }
    };
}
macro_rules! grf_rem {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_rem($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    return Err(graphite_core::diagnostic::remainder_by_zero($site));
                } else {
                    return Err(graphite_core::diagnostic::integer_overflow($site));
                }
            }
        }
    };
}
macro_rules! grf_neg {
    ($a:expr, $site:expr) => {
        match $a.checked_neg() {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_pow {
    ($a:expr, $b:expr, $site:expr) => {{
        let grf_exp = match u32::try_from($b) {
            Ok(exp) => exp,
            Err(_) => return Err(graphite_core::diagnostic::integer_overflow($site)),
        };
        match $a.checked_pow(grf_exp) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    }};
}
macro_rules! grf_floordiv {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_div_euclid($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    return Err(graphite_core::diagnostic::division_by_zero($site));
                } else {
                    return Err(graphite_core::diagnostic::integer_overflow($site));
                }
            }
        }
    };
}

pub(crate) fn grf_str_length(value: &str) -> u64 {
    value.chars().count() as u64
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GrfType {
    pub(crate) name: &'static str,
    pub(crate) record: bool,
}

struct GrfClassAnimal {
    grf_type: u32,
    Animal_name: String,
}
impl graphite_core::heap::GcObject for GrfClassAnimal {
    fn trace(&self, visit: &mut dyn FnMut(graphite_core::heap::Handle)) {
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn grf_type_id(&self) -> u32 { self.grf_type }
}



struct GrfClassAnimalFactory {
    grf_type: u32,
}
impl graphite_core::heap::GcObject for GrfClassAnimalFactory {
    fn trace(&self, visit: &mut dyn FnMut(graphite_core::heap::Handle)) {
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn grf_type_id(&self) -> u32 { self.grf_type }
}

struct GrfClassReport {
    grf_type: u32,
    title: String,
}
impl graphite_core::heap::GcObject for GrfClassReport {
    fn trace(&self, visit: &mut dyn FnMut(graphite_core::heap::Handle)) {
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn grf_type_id(&self) -> u32 { self.grf_type }
}

struct GrfClassFormatter {
    grf_type: u32,
}
impl graphite_core::heap::GcObject for GrfClassFormatter {
    fn trace(&self, visit: &mut dyn FnMut(graphite_core::heap::Handle)) {
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn grf_type_id(&self) -> u32 { self.grf_type }
}

struct GrfClassCounter {
    grf_type: u32,
}
impl graphite_core::heap::GcObject for GrfClassCounter {
    fn trace(&self, visit: &mut dyn FnMut(graphite_core::heap::Handle)) {
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn grf_type_id(&self) -> u32 { self.grf_type }
}

fn grf_new_Animal(mut name: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let this = graphite_core::heap::alloc(GrfClassAnimal {
        grf_type: 0,
        Animal_name: String::new(),
    })?;
    grf_init_Animal(this, name)
}

fn grf_init_Animal(this: graphite_core::heap::Rooted, mut name: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let grf_stored = String::from("");
    let grf_packed = grf_stored.clone();
    graphite_core::heap::with_mut::<GrfClassAnimal, ()>(this.handle(), graphite_core::convert::Site { file: "", line: 0, column: 1, end_column: 1, excerpt: "" }, |obj| { obj.Animal_name = grf_packed; })?;
    let _ = { let grf_obj = this.clone(); let grf_stored = name.clone(); let grf_packed = grf_stored.clone(); graphite_core::heap::with_mut::<GrfClassAnimal, ()>(grf_obj.handle(), graphite_core::convert::Site { file: "examples/oop/main.grf", line: 10, column: 9, end_column: 18, excerpt: "        this.name = name;" }, |obj| { obj.Animal_name = grf_packed; })?;  grf_stored };
    Ok(this)
}

fn grf_virt_Animal_speak(this: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
    let grf_tag = graphite_core::heap::type_id(this.handle())?;
    match grf_tag {
        1 => crate::grf_method_Dog_speak(this),
        2 => crate::grf_method_Cat_speak(this),
        _ => Err(graphite_core::diagnostic::runtime_internal("abstract method")),
    }
}

fn grf_method_Animal_introduce(this: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
    match graphite_core::io::println({ let mut grf_text = String::new(); grf_text.push_str("My name is "); grf_text.push_str(&({ let grf_obj = this.clone(); let grf_raw = graphite_core::heap::with::<GrfClassAnimal, _>(grf_obj.handle(), |obj| obj.Animal_name.clone())?; grf_raw })); grf_text }) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 17, column: 9, end_column: 46, excerpt: "        io.println(f\"My name is {this.name}\");" })) };
    Ok(())
}

fn grf_new_Dog(mut name: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let this = graphite_core::heap::alloc(GrfClassAnimal {
        grf_type: 1,
        Animal_name: String::new(),
    })?;
    grf_init_Dog(this, name)
}

fn grf_init_Dog(this: graphite_core::heap::Rooted, mut name: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let _grf_parent = grf_init_Animal(this.clone(), name.clone())?;
    Ok(this)
}

fn grf_method_Dog_speak(this: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
    match graphite_core::io::println({ let mut grf_text = String::new(); grf_text.push_str(&({ let grf_obj = this.clone(); let grf_raw = graphite_core::heap::with::<GrfClassAnimal, _>(grf_obj.handle(), |obj| obj.Animal_name.clone())?; grf_raw })); grf_text.push_str(" says woof!"); grf_text }) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 28, column: 9, end_column: 46, excerpt: "        io.println(f\"{this.name} says woof!\");" })) };
    Ok(())
}

fn grf_new_Cat(mut name: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let this = graphite_core::heap::alloc(GrfClassAnimal {
        grf_type: 2,
        Animal_name: String::new(),
    })?;
    grf_init_Cat(this, name)
}

fn grf_init_Cat(this: graphite_core::heap::Rooted, mut name: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let _grf_parent = grf_init_Animal(this.clone(), name.clone())?;
    Ok(this)
}

fn grf_method_Cat_speak(this: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
    match graphite_core::io::println({ let mut grf_text = String::new(); grf_text.push_str(&({ let grf_obj = this.clone(); let grf_raw = graphite_core::heap::with::<GrfClassAnimal, _>(grf_obj.handle(), |obj| obj.Animal_name.clone())?; grf_raw })); grf_text.push_str(" says meow!"); grf_text }) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 39, column: 9, end_column: 46, excerpt: "        io.println(f\"{this.name} says meow!\");" })) };
    Ok(())
}

fn grf_new_AnimalFactory() -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let this = graphite_core::heap::alloc(GrfClassAnimalFactory {
        grf_type: 3,
    })?;
    grf_init_AnimalFactory(this)
}

fn grf_init_AnimalFactory(this: graphite_core::heap::Rooted) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    Ok(this)
}

fn grf_method_AnimalFactory_describe(mut animal: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
    match crate::grf_method_Animal_introduce(animal.clone()) { Ok(value) => value, Err(error) => return Err(error.enter_call("introduce", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 46, column: 9, end_column: 27, excerpt: "        animal.introduce();" })) };
    match crate::grf_virt_Animal_speak(animal.clone()) { Ok(value) => value, Err(error) => return Err(error.enter_call("speak", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 47, column: 9, end_column: 23, excerpt: "        animal.speak();" })) };
    Ok(())
}

fn grf_new_Report(mut title: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let this = graphite_core::heap::alloc(GrfClassReport {
        grf_type: 4,
        title: String::new(),
    })?;
    grf_init_Report(this, title)
}

fn grf_init_Report(this: graphite_core::heap::Rooted, mut title: String) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let grf_stored = String::from("");
    let grf_packed = grf_stored.clone();
    graphite_core::heap::with_mut::<GrfClassReport, ()>(this.handle(), graphite_core::convert::Site { file: "", line: 0, column: 1, end_column: 1, excerpt: "" }, |obj| { obj.title = grf_packed; })?;
    let _ = { let grf_obj = this.clone(); let grf_stored = title.clone(); let grf_packed = grf_stored.clone(); graphite_core::heap::with_mut::<GrfClassReport, ()>(grf_obj.handle(), graphite_core::convert::Site { file: "examples/oop/main.grf", line: 59, column: 9, end_column: 19, excerpt: "        this.title = title;" }, |obj| { obj.title = grf_packed; })?;  grf_stored };
    Ok(this)
}

fn grf_method_Report_print(this: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
    match graphite_core::io::println({ let grf_obj = this.clone(); let grf_raw = graphite_core::heap::with::<GrfClassReport, _>(grf_obj.handle(), |obj| obj.title.clone())?; grf_raw }) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 64, column: 9, end_column: 31, excerpt: "        io.println(this.title);" })) };
    Ok(())
}

fn grf_new_Formatter() -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let this = graphite_core::heap::alloc(GrfClassFormatter {
        grf_type: 5,
    })?;
    grf_init_Formatter(this)
}

fn grf_init_Formatter(this: graphite_core::heap::Rooted) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    Ok(this)
}

fn grf_method_Formatter_format_0(this: graphite_core::heap::Rooted, mut value: i64) -> Result<String, graphite_core::diagnostic::RuntimeError> {
    return Ok(String::from("Integer"));
    unreachable!();
}

fn grf_method_Formatter_format_1(this: graphite_core::heap::Rooted, mut value: String) -> Result<String, graphite_core::diagnostic::RuntimeError> {
    return Ok(value.clone());
    unreachable!();
}

thread_local! {
    static GRF_SF_Counter_count: std::cell::RefCell<Option<i64>> = const { std::cell::RefCell::new(None) };
}
fn grf_sget_Counter_count() -> Result<i64, graphite_core::diagnostic::RuntimeError> {
    if let Some(value) = GRF_SF_Counter_count.with(|slot| slot.borrow().clone()) { return Ok(value); }
    let value = 0i64;
    GRF_SF_Counter_count.with(|slot| *slot.borrow_mut() = Some(value.clone()));
    Ok(value)
}
fn grf_sset_Counter_count(value: i64) -> Result<i64, graphite_core::diagnostic::RuntimeError> {
    GRF_SF_Counter_count.with(|slot| *slot.borrow_mut() = Some(value.clone()));
    Ok(value)
}

fn grf_new_Counter() -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let this = graphite_core::heap::alloc(GrfClassCounter {
        grf_type: 6,
    })?;
    grf_init_Counter(this)
}

fn grf_init_Counter(this: graphite_core::heap::Rooted) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {
    let _ = { let _grf_recv = false; let grf_stored = grf_add!({ let _grf_recv = false; crate::grf_sget_Counter_count()? }, 1i64, graphite_core::convert::Site { file: "examples/oop/main.grf", line: 83, column: 25, end_column: 42, excerpt: "        Counter.count = Counter.count + 1;" }); crate::grf_sset_Counter_count(grf_stored.clone())?;  grf_stored };
    Ok(this)
}

fn grf_method_Counter_get_count() -> Result<i64, graphite_core::diagnostic::RuntimeError> {
    return Ok({ let _grf_recv = false; crate::grf_sget_Counter_count()? });
    unreachable!();
}

fn grf_iface_Printable_print_0(this: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
    let grf_tag = graphite_core::heap::type_id(this.handle())?;
    match grf_tag {
        4 => crate::grf_method_Report_print(this),
        _ => Err(graphite_core::diagnostic::runtime_internal("interface method is not implemented")),
    }
}

mod grf_fns {
    use super::*;
    pub(crate) fn show(mut item: graphite_core::heap::Rooted) -> Result<(), graphite_core::diagnostic::RuntimeError> {
        match crate::grf_iface_Printable_print_0(item.clone()) { Ok(value) => value, Err(error) => return Err(error.enter_call("print", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 93, column: 5, end_column: 17, excerpt: "    item.print();" })) };
        Ok(())
    }

}

fn grf_user_main() -> Result<(), graphite_core::diagnostic::RuntimeError> {
    let mut dog: graphite_core::heap::Rooted = match crate::grf_new_Dog(String::from("Buddy")) { Ok(value) => value, Err(error) => return Err(error.enter_call("Dog", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 97, column: 15, end_column: 27, excerpt: "    Dog dog = Dog(\"Buddy\");" })) };
    let mut cat: graphite_core::heap::Rooted = match crate::grf_new_Cat(String::from("Luna")) { Ok(value) => value, Err(error) => return Err(error.enter_call("Cat", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 98, column: 15, end_column: 26, excerpt: "    Cat cat = Cat(\"Luna\");" })) };
    match crate::grf_method_AnimalFactory_describe(dog.clone()) { Ok(value) => value, Err(error) => return Err(error.enter_call("describe", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 99, column: 5, end_column: 32, excerpt: "    AnimalFactory.describe(dog);" })) };
    match crate::grf_method_AnimalFactory_describe(cat.clone()) { Ok(value) => value, Err(error) => return Err(error.enter_call("describe", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 100, column: 5, end_column: 32, excerpt: "    AnimalFactory.describe(cat);" })) };
    match crate::grf_fns::show(match crate::grf_new_Report(String::from("Annual Report")) { Ok(value) => value, Err(error) => return Err(error.enter_call("Report", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 101, column: 10, end_column: 33, excerpt: "    show(Report(\"Annual Report\"));" })) }) { Ok(value) => value, Err(error) => return Err(error.enter_call("show", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 101, column: 5, end_column: 34, excerpt: "    show(Report(\"Annual Report\"));" })) };
    let mut formatter: graphite_core::heap::Rooted = match crate::grf_new_Formatter() { Ok(value) => value, Err(error) => return Err(error.enter_call("Formatter", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 102, column: 27, end_column: 38, excerpt: "    Formatter formatter = Formatter();" })) };
    match graphite_core::io::println(match crate::grf_method_Formatter_format_0(formatter.clone(), 10i64) { Ok(value) => value, Err(error) => return Err(error.enter_call("format", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 103, column: 16, end_column: 36, excerpt: "    io.println(formatter.format(10));" })) }) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 103, column: 5, end_column: 37, excerpt: "    io.println(formatter.format(10));" })) };
    match graphite_core::io::println(match crate::grf_method_Formatter_format_1(formatter.clone(), String::from("Hello")) { Ok(value) => value, Err(error) => return Err(error.enter_call("format", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 104, column: 16, end_column: 41, excerpt: "    io.println(formatter.format(\"Hello\"));" })) }) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 104, column: 5, end_column: 42, excerpt: "    io.println(formatter.format(\"Hello\"));" })) };
    let mut first: graphite_core::heap::Rooted = match crate::grf_new_Counter() { Ok(value) => value, Err(error) => return Err(error.enter_call("Counter", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 105, column: 21, end_column: 30, excerpt: "    Counter first = Counter();" })) };
    let mut second: graphite_core::heap::Rooted = match crate::grf_new_Counter() { Ok(value) => value, Err(error) => return Err(error.enter_call("Counter", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 106, column: 22, end_column: 35, excerpt: "    Counter second = new Counter();" })) };
    match graphite_core::io::println(graphite_core::convert::signed_text(i128::from(match crate::grf_method_Counter_get_count() { Ok(value) => value, Err(error) => return Err(error.enter_call("get_count", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 107, column: 16, end_column: 35, excerpt: "    io.println(Counter.get_count());" })) }))) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 107, column: 5, end_column: 36, excerpt: "    io.println(Counter.get_count());" })) };
    match graphite_core::io::println(graphite_core::convert::signed_text(i128::from({ let _grf_recv = first.clone(); match crate::grf_method_Counter_get_count() { Ok(value) => value, Err(error) => return Err(error.enter_call("get_count", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 108, column: 16, end_column: 33, excerpt: "    io.println(first.get_count());" })) } }))) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 108, column: 5, end_column: 34, excerpt: "    io.println(first.get_count());" })) };
    match graphite_core::io::println(graphite_core::convert::signed_text(i128::from({ let _grf_recv = second.clone(); match crate::grf_method_Counter_get_count() { Ok(value) => value, Err(error) => return Err(error.enter_call("get_count", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 109, column: 16, end_column: 34, excerpt: "    io.println(second.get_count());" })) } }))) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/oop/main.grf", line: 109, column: 5, end_column: 35, excerpt: "    io.println(second.get_count());" })) };
    Ok(())
}

fn main() {
    if let Err(error) = grf_user_main() {
        graphite_core::diagnostic::report(&error);
        std::process::exit(1);
    }
}

