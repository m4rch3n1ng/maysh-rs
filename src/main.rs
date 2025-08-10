use gix::{Id, Repository, bstr::BString, hash::Prefix};
use owo_colors::OwoColorize;
use std::{
	env::current_dir,
	fmt::{Display, Write},
	fs,
	path::{Path, PathBuf},
};

fn trim_in_place(mut string: String) -> String {
	let trimmed = string.trim_end();
	string.truncate(trimmed.len());
	string
}

fn head_name(string: String) -> String {
	let st = string.replace("refs/heads/", "");
	trim_in_place(st)
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

impl Display for Head {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Head::Branch(branch) => write!(f, "{branch}"),
			Head::Commit(hash) => write!(f, ":{hash}"),
		}
	}
}

fn branch(repo: &Repository) -> Head {
	let head = repo.head().unwrap();
	let branch = head.referent_name();

	match branch {
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

#[derive(Debug)]
#[must_use]
enum Mode {
	ApplyMailbox,
	Rebase,
	AmRbs,
	RebaseInt(Option<String>, Option<(String, String)>),
	Bisect(Option<String>),
	Merge(Prefix),
	CherryPick(Prefix),
	Revert(Prefix),
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

				if let Some((sta, end)) = status {
					write!(f, " {sta}/{end}")?;
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

// thx https://github.com/Byron/gitoxide/blob/31801420e1bef1ebf32e14caf73ba29ddbc36443/gix/src/repository/state.rs#L3
// thx https://github.com/Byron/gitoxide/blob/31801420e1bef1ebf32e14caf73ba29ddbc36443/gix/src/state.rs#L3
fn mode(repo: &Repository, path: &Path) -> Option<Mode> {
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

		let branch = fs::read_to_string(path.join("head-name"))
			.ok()
			.map(head_name);
		let status = fs::read_to_string(path.join("msgnum"))
			.ok()
			.map(trim_in_place)
			.zip(fs::read_to_string(path.join("end")).ok().map(trim_in_place));

		Some(Mode::RebaseInt(branch, status))
	} else if path.join("BISECT_LOG").is_file() {
		let branch = fs::read_to_string(path.join("BISECT_START"))
			.ok()
			.map(trim_in_place);
		Some(Mode::Bisect(branch))
	} else if let Ok(sha) = fs::read_to_string(path.join("MERGE_HEAD")) {
		let hash = hash(repo, &sha);
		Some(Mode::Merge(hash))
	} else if let Ok(sha) = fs::read_to_string(path.join("CHERRY_PICK_HEAD")) {
		let hash = hash(repo, &sha);
		Some(Mode::CherryPick(hash))
	} else if let Ok(sha) = fs::read_to_string(path.join("REVERT_HEAD")) {
		let hash = hash(repo, &sha);
		Some(Mode::Revert(hash))
	} else {
		None
	}
}

fn git() -> Result<String, Box<dyn std::error::Error>> {
	let repo = gix::discover(".")?;
	let branch = branch(&repo);

	let path = repo.path();
	let mode = mode(&repo, path);

	let mut string = String::new();
	write!(string, "{}", "(".green())?;

	if let Some(mode) = mode {
		write!(string, "{} ", mode.red())?;
	}

	write!(string, "{}{}", branch.green(), ")".green())?;
	Ok(string)
}

enum Start {
	Root,
	User,
}

impl Display for Start {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Start::Root => write!(f, "{}", "#".bold().red()),
			Start::User => write!(f, "$"),
		}
	}
}

fn start() -> Start {
	match std::env::var("USER").as_deref() {
		Ok("root") => Start::Root,
		_ => Start::User,
	}
}

#[repr(transparent)]
struct Dir(PathBuf);

impl Display for Dir {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if let Some(name) = self.0.file_name() {
			let path = Path::new(name);
			write!(f, "{}", path.display().cyan())
		} else {
			write!(f, "{}", self.0.display().cyan())
		}
	}
}

fn dir() -> Dir {
	let path = current_dir().unwrap_or_default();
	Dir(path)
}

#[repr(transparent)]
struct User(String);

impl Display for User {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0.yellow())
	}
}

fn usr() -> User {
	let env = std::env::var("USER").unwrap_or_default();
	User(env)
}

fn main() {
	let start = start();
	let usr = usr();
	let dir = dir();

	if let Ok(git) = git() {
		print!("{start} {usr} {dir} {git} >> ");
	} else {
		print!("{start} {usr} {dir} >> ");
	}
}
