use std::{path::PathBuf, str::FromStr, sync::Arc};

use jj_lib::revset::RevsetExpression;
use pollster::FutureExt;

fn main() {
    let repo = std::env::args().nth(1).unwrap();
    let bare = std::env::args()
        .nth(2)
        .map(|s| s == "--bare")
        .unwrap_or(false);

    let repo_dir = if !bare {
        PathBuf::from_str(&format!("/home/jj/jj/{}/.jj/repo", repo)).unwrap()
    } else {
        PathBuf::from_str(&format!("/home/jj/jj/{}", repo)).unwrap()
    };

    let repo = jj::repo_loader(&repo_dir).unwrap();
    let repo = repo.load_at_head().block_on().unwrap();

    let expression = Arc::new(RevsetExpression::All);
    let revset = expression.evaluate(repo.as_ref()).unwrap();

    let commit_count = revset.count_estimate().unwrap().0;

    println!("Successfully loaded the repo, commits: {commit_count}")
}
