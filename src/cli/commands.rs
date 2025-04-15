use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>, 
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    
    Configure(ConfigureArgs),
    
    Ask { prompt: String },
    
    Generate(GenerateArgs),
    
    Explain(ExplainArgs),
    
    Edit(EditArgs),
    Debug(DebugArgs),
    
    Test(TestArgs),
    
    Doc(DocArgs),
    
    Run(RunArgs),
    
    Shell(ShellArgs),
   }
   
   #[derive(Args, Debug)]
   pub struct ConfigureArgs {
    
    
    #[arg(long, value_name = "KEY_ENTRY_NAME")]
    pub set_api_key: Option<Option<String>>, 

    
    #[arg(long, value_name = "MODEL_ID")]
    pub set_default_model: Option<String>,

    
    #[arg(long, value_name = "MODEL_ID")]
    pub set_edit_model: Option<String>,
}

#[derive(Args, Debug)]
pub struct GenerateArgs {
    
    pub description: String,

    
    #[arg(long, value_name = "FILE_PATH")]
    pub file: Option<String>,
}

#[derive(Args, Debug)]
#[group(required = false, multiple = false)] 
pub struct ExplainArgs {
    
    #[arg(long, required = true)]
    pub file: String,

    
    #[arg(long, group = "context_specifier")]
    pub lines: Option<String>,

    
    #[arg(long, group = "context_specifier")]
    pub symbol: Option<String>,
}


#[derive(Args, Debug)]
pub struct EditArgs {
	
	pub instruction: String,

	
	#[arg(long, required = true)]
	pub file: String,
}


#[derive(Args, Debug)]
pub struct DebugArgs {
    
    #[arg(long, required = true)]
    pub error: String,

    
    #[arg(long, value_name = "FILE_PATH")]
    pub file: Option<String>,
}

#[derive(Args, Debug)]
pub struct TestArgs {
    
    #[arg(long, required = true)]
    pub file: String,
}


#[derive(Args, Debug)]
pub struct DocArgs {
    
    #[arg(long, required = true)]
    pub file: String,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    
    pub task_description: String,
}

#[derive(Args, Debug)]
pub struct ShellArgs {
    #[command(subcommand)]
    pub command: ShellCommands,
}

#[derive(Subcommand, Debug)]
pub enum ShellCommands {
    
    Explain(ShellExplainArgs),
    
    Suggest(ShellSuggestArgs),
}

#[derive(Args, Debug)]
pub struct ShellExplainArgs {
    
    pub command_string: String,
}

#[derive(Args, Debug)]
pub struct ShellSuggestArgs {
    
    pub description: String,
}