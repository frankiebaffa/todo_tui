use {
    crate::args::{ Args, Mode, },
    std::path::PathBuf,
    todo_core::GetPath,
};
#[derive(Clone)]
pub struct Ctx {
    pub args: Args,
    path: PathBuf,
}
impl Ctx {
    pub fn new(args: Args) -> Self {
        Self { args, path: PathBuf::new(), }
    }
    pub fn construct_path(&mut self) {
        let path = match &self.args.mode {
            Mode::Open(args) => {
                &args.list_path
            },
            Mode::New(args) => {
                &args.list_path
            },
        };
        let tmp_path = PathBuf::from(format!("{}", &path));
        match tmp_path.extension() {
            Some(ext) => {
                if !ext.eq("json") {
                    self.path.push(format!("{}.json", &path));
                } else {
                    self.path.push(format!("{}", &path));
                }
            },
            None => self.path.push(format!("{}.json", &path)),
        }
    }
}
impl GetPath for Ctx {
    fn get_path(&self) -> &PathBuf {
        &self.path
    }
    fn get_path_mut(&mut self) -> &mut PathBuf {
        &mut self.path
    }
}
