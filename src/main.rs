use gix::{Id, Repository, bstr::BString, hash::Prefix};
use owo_colors::OwoColorize;
use std::{
	fmt::{Display, Write},
	path::{Path, PathBuf},
};

fn trim_in_place(mut string: String) -> String {
	let trimmed = string.trim_end();
	string.truncate(trimmed.len());
	string
}

// https://github.com/Byron/gitoxide/issues/1268
fn rel(rev: Id) -> Prefix {
	rev.shorten().unwrap()
}

fn hash(repo: &Repository, hash: &str) -> Prefix {
	let hash = hash.trim();
	let hash = repo.rev_parse_single(hash).unwrap();
	rel(hash)
}

#[derive(Debug)]
enum Head {
	Branch(BString),
	Commit(Prefix),
}

impl Head {
	fn new(repo: &Repository) -> Self {
		let head = repo.head().unwrap();
		match head.referent_name() {
			Some(branch) => {
				let branch = branch.shorten();
				Head::Branch(branch.to_owned())
			}
			None => {
				let hash = head.id().unwrap();
				let hash = rel(hash);
				Head::Commit(hash)
			}
		}
	}
}

impl Display for Head {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Head::Branch(branch) => write!(f, "{branch}"),
			Head::Commit(hash) => write!(f, ":{hash}"),
		}
	}
}

#[derive(Debug)]
struct Status {
	num: String,
	end: String,
}

impl Display for Status {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}/{}", self.num, self.end)
	}
}

#[derive(Debug)]
#[must_use]
enum Mode {
	ApplyMailbox,
	Rebase,
	AmRbs,
	RebaseInt(Option<Head>, Option<Status>),
	Bisect(Option<String>),
	Merge(Prefix),
	CherryPick(Prefix),
	Revert(Prefix),
}

impl Mode {
	// thx https://github.com/Byron/gitoxide/blob/31801420e1bef1ebf32e14caf73ba29ddbc36443/gix/src/repository/state.rs#L3
	// thx https://github.com/Byron/gitoxide/blob/31801420e1bef1ebf32e14caf73ba29ddbc36443/gix/src/state.rs#L3
	fn new(repo: &Repository, path: &Path) -> Option<Mode> {
		if path.join("rebase-apply/applying").is_file() {
			Some(Mode::ApplyMailbox)
		} else if path.join("rebase-apply/rebasing").is_file() {
			// todo rebase steps / extra info ?
			// idk how to get into this mode lol
			Some(Mode::Rebase)
		} else if path.join("rebase-apply").is_dir() {
			Some(Mode::AmRbs)
		} else if path.join("rebase-merge").is_dir() {
			let path = path.join("rebase-merge");

			let branch = if let Ok(head) = std::fs::read_to_string(path.join("head-name"))
				&& let Some(head) = head.strip_prefix("refs/heads/")
			{
				let branch = BString::from(head.trim_end());
				Some(Head::Branch(branch))
			} else if let Ok(head) = std::fs::read_to_string(path.join("orig-head")) {
				let hash = hash(repo, &head);
				Some(Head::Commit(hash))
			} else {
				None
			};

			let status = if let Ok(num) = std::fs::read_to_string(path.join("msgnum"))
				&& let Ok(end) = std::fs::read_to_string(path.join("end"))
			{
				let status = Status {
					num: trim_in_place(num),
					end: trim_in_place(end),
				};
				Some(status)
			} else {
				None
			};

			Some(Mode::RebaseInt(branch, status))
		} else if path.join("BISECT_LOG").is_file() {
			let branch = std::fs::read_to_string(path.join("BISECT_START"))
				.ok()
				.map(trim_in_place);

			Some(Mode::Bisect(branch))
		} else if let Ok(sha) = std::fs::read_to_string(path.join("MERGE_HEAD")) {
			let hash = hash(repo, &sha);
			Some(Mode::Merge(hash))
		} else if let Ok(sha) = std::fs::read_to_string(path.join("CHERRY_PICK_HEAD")) {
			let hash = hash(repo, &sha);
			Some(Mode::CherryPick(hash))
		} else if let Ok(sha) = std::fs::read_to_string(path.join("REVERT_HEAD")) {
			let hash = hash(repo, &sha);
			Some(Mode::Revert(hash))
		} else {
			None
		}
	}
}

impl Display for Mode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match *self {
			Mode::ApplyMailbox => write!(f, "am"),
			Mode::Rebase => write!(f, "rbs"),
			Mode::AmRbs => write!(f, "am/rbs"),
			Mode::RebaseInt(ref branch, ref status) => {
				write!(f, "rbs")?;

				if let Some(branch) = branch {
					write!(f, " {branch}")?;
				}

				if let Some(status) = status {
					write!(f, " {status}")?;
				}

				Ok(())
			}
			Mode::Bisect(ref branch) => {
				write!(f, "bsc")?;

				if let Some(branch) = branch {
					write!(f, " {branch}")?;
				}

				Ok(())
			}
			Mode::Merge(ref hash) => {
				write!(f, "mrg :{hash}")
			}
			Mode::CherryPick(ref hash) => {
				write!(f, "chp :{hash}")
			}
			Mode::Revert(ref hash) => {
				write!(f, "rvt :{hash}")
			}
		}
	}
}

fn git() -> Result<String, Box<dyn std::error::Error>> {
	let repo = gix::discover(".")?;
	let branch = Head::new(&repo);

	let mut string = String::new();
	write!(string, "{}", "(".green())?;

	let path = repo.path();
	if let Some(mode) = Mode::new(&repo, path) {
		write!(string, "{} ", mode.red())?;
	}

	write!(string, "{}{}", branch.green(), ")".green())?;
	Ok(string)
}

enum Start {
	Root,
	User,
}

impl Start {
	fn new(usr: &User) -> Self {
		match &*usr.0 {
			"root" => Start::Root,
			_ => Start::User,
		}
	}
}

impl Display for Start {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Start::Root => write!(f, "{}", "#".bold().red()),
			Start::User => write!(f, "$"),
		}
	}
}

#[repr(transparent)]
struct Dir(PathBuf);

impl Dir {
	fn cwd() -> Self {
		let path = std::env::current_dir().unwrap_or_default();
		Dir(path)
	}
}

impl Display for Dir {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if let Some(name) = self.0.file_name() {
			write!(f, "{}", name.display().cyan())
		} else {
			write!(f, "{}", self.0.display().cyan())
		}
	}
}

#[repr(transparent)]
struct User(String);

impl User {
	fn current() -> Self {
		let env = std::env::var("USER").unwrap_or_default();
		User(env)
	}
}

impl Display for User {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0.yellow())
	}
}

fn main() {
	let usr = User::current();
	let start = Start::new(&usr);
	let dir = Dir::cwd();

	if let Ok(git) = git() {
		print!("{start} {usr} {dir} {git} >> ");
	} else {
		print!("{start} {usr} {dir} >> ");
	}
}
