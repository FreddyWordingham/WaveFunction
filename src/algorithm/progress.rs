use indicatif::{ProgressBar, ProgressStyle};
use ndarray::Array2;

/// Progress bar tracking for WFC algorithms
pub struct WfcProgress {
    progress_bar: ProgressBar,
    backtrack_count: usize,
}

impl WfcProgress {
    /// Creates a new progress tracker for standard WFC
    pub fn new(cells_to_collapse: usize, with_backtracking: bool) -> Self {
        let pb = ProgressBar::new(cells_to_collapse as u64);

        // Use different style based on algorithm type
        if with_backtracking {
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} cells (Backtracked: {msg})"
                )
                .unwrap()
                .progress_chars("##-"),
            );
            pb.set_message("0");
        } else {
            pb.set_style(
                ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} cells")
                    .unwrap()
                    .progress_chars("##-"),
            );
        }

        Self {
            progress_bar: pb,
            backtrack_count: 0,
        }
    }

    /// Count cells requiring collapsing
    pub fn count_cells_to_collapse(
        domain_sizes: &Array2<usize>,
        is_ignore: &Array2<bool>,
    ) -> usize {
        let (height, width) = domain_sizes.dim();
        let mut count = 0;

        for y in 0..height {
            for x in 0..width {
                if !is_ignore[(y, x)] && domain_sizes[(y, x)] > 1 {
                    count += 1;
                }
            }
        }

        count
    }

    /// Increment progress
    pub fn increment(&self) {
        self.progress_bar.inc(1);
    }

    /// Record a backtrack event
    pub fn record_backtrack(&mut self) {
        self.backtrack_count += 1;
        self.progress_bar
            .set_message(self.backtrack_count.to_string());
    }

    /// Get current backtrack count
    pub fn backtrack_count(&self) -> usize {
        self.backtrack_count
    }

    /// Print a message through the progress bar
    pub fn println(&self, message: String) {
        self.progress_bar.println(message);
    }

    /// Finish and clear progress display
    pub fn finish(self) {
        self.progress_bar.finish_and_clear();

        if self.backtrack_count > 0 {
            println!(
                "Completed with {} backtracking attempts",
                self.backtrack_count
            );
        }
    }
}
