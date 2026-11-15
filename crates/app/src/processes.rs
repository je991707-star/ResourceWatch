//! Process table sorting. Rows keep their raw values here; the table shows
//! formatted text. A missing CPU value (first scan) always sorts last.

use resourcewatch_core::collectors::process::ProcessInfo;
use std::cmp::Ordering;

/// Column indices as used by `ProcessTable` in `ui/table.slint`.
pub const COL_NAME: i32 = 0;
pub const COL_PID: i32 = 1;
pub const COL_CPU: i32 = 2;
// 3 (and any other value) = memory.

pub fn sort(list: &mut [ProcessInfo], column: i32, ascending: bool) {
    list.sort_by(|a, b| {
        let dir = |o: Ordering| if ascending { o } else { o.reverse() };
        let primary = match column {
            COL_NAME => dir(a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            COL_PID => dir(a.pid.cmp(&b.pid)),
            COL_CPU => match (a.cpu, b.cpu) {
                (Some(x), Some(y)) => dir(x.total_cmp(&y)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            _ => dir(a.memory.cmp(&b.memory)), // memory column
        };
        // Stable, readable tie-break: by PID.
        primary.then(a.pid.cmp(&b.pid))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str, cpu: Option<f64>, memory: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: name.into(),
            cpu,
            memory,
        }
    }

    fn pids(list: &[ProcessInfo]) -> Vec<u32> {
        list.iter().map(|x| x.pid).collect()
    }

    #[test]
    fn sorts_each_column_with_missing_cpu_last() {
        let mut l = vec![
            p(3, "bash", Some(5.0), 300),
            p(1, "Xorg", None, 100),
            p(2, "kwin", Some(20.0), 200),
            p(4, "bash", Some(5.0), 50),
        ];
        sort(&mut l, COL_CPU, false);
        assert_eq!(pids(&l), [2, 3, 4, 1], "descending, tie by pid, None last");
        sort(&mut l, COL_CPU, true);
        assert_eq!(pids(&l), [3, 4, 2, 1], "None last in both directions");
        sort(&mut l, COL_NAME, true);
        assert_eq!(pids(&l), [3, 4, 2, 1], "case-insensitive name");
        sort(&mut l, 3, false);
        assert_eq!(pids(&l), [3, 2, 1, 4]);
        sort(&mut l, COL_PID, true);
        assert_eq!(pids(&l), [1, 2, 3, 4]);
    }
}
