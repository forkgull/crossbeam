//! Loom-based testing.

#![cfg(crossbeam_loom)]

mod array_queue {
    use crossbeam_queue::ArrayQueue;

    use loom_crate::sync::atomic::{AtomicUsize, Ordering};
    use loom_crate::sync::Arc;
    use loom_crate::thread::spawn;

    #[test]
    fn spsc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 100_000;

        loom_crate::model(|| {
            let q = Arc::new(ArrayQueue::new(3));

            spawn({
                let q = q.clone();
                move || {
                    for i in 0..COUNT {
                        loop {
                            if let Some(x) = q.pop() {
                                assert_eq!(x, i);
                                break;
                            }
                        }
                    }
                    assert!(q.pop().is_none());
                }
            });

            spawn(move || {
                for i in 0..COUNT {
                    while q.push(i).is_err() {}
                }
            });
        });
    }

    #[test]
    fn spsc_ring_buffer() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 100_000;

        loom_crate::model(|| {
            let t = AtomicUsize::new(1);
            let q = ArrayQueue::<usize>::new(3);
            let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();
            let state = Arc::new((t, q, v));

            spawn({
                let state = state.clone();
                move || {
                    let (t, q, v) = &*state;
                    loop {
                        match t.load(Ordering::SeqCst) {
                            0 if q.is_empty() => break,

                            _ => {
                                while let Some(n) = q.pop() {
                                    v[n].fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }
                    }
                }
            });

            spawn({
                let state = state.clone();
                move || {
                    let (t, q, v) = &*state;
                    for i in 0..COUNT {
                        if let Some(n) = q.force_push(i) {
                            v[n].fetch_add(1, Ordering::SeqCst);
                        }
                    }

                    t.fetch_sub(1, Ordering::SeqCst);
                }
            });

            let (_, _, v) = &*state;
            for c in v.iter() {
                assert_eq!(c.load(Ordering::SeqCst), 1);
            }
        });
    }

    #[test]
    fn mpmc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 25_000;
        const THREADS: usize = 4;

        loom_crate::model(|| {
            let q = ArrayQueue::<usize>::new(3);
            let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();
            let state = Arc::new((q, v));

            for _ in 0..THREADS {
                scope.spawn(|_| {
                    for _ in 0..COUNT {
                        let n = loop {
                            if let Some(x) = q.pop() {
                                break x;
                            }
                        };
                        v[n].fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
            for _ in 0..THREADS {
                scope.spawn(|_| {
                    for i in 0..COUNT {
                        while q.push(i).is_err() {}
                    }
                });
            }

            for c in v {
                assert_eq!(c.load(Ordering::SeqCst), THREADS);
            }
        });
    }

    #[test]
    fn mpmc_ring_buffer() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 25_000;
        const THREADS: usize = 4;

        loom_crate::model(|| {
            let t = AtomicUsize::new(THREADS);
            let q = ArrayQueue::<usize>::new(3);
            let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

            scope(|scope| {
                for _ in 0..THREADS {
                    scope.spawn(|_| loop {
                        match t.load(Ordering::SeqCst) {
                            0 if q.is_empty() => break,

                            _ => {
                                while let Some(n) = q.pop() {
                                    v[n].fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }
                    });
                }

                for _ in 0..THREADS {
                    scope.spawn(|_| {
                        for i in 0..COUNT {
                            if let Some(n) = q.force_push(i) {
                                v[n].fetch_add(1, Ordering::SeqCst);
                            }
                        }

                        t.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            })
            .unwrap();

            for c in v {
                assert_eq!(c.load(Ordering::SeqCst), THREADS);
            }
        });
    }

    #[test]
    fn drops() {
        let runs: usize = if cfg!(miri) { 3 } else { 100 };
        let steps: usize = if cfg!(miri) { 50 } else { 10_000 };
        let additional: usize = if cfg!(miri) { 10 } else { 50 };

        loom_crate::model(|| {
            static DROPS: AtomicUsize = AtomicUsize::new(0);

            #[derive(Debug, PartialEq)]
            struct DropCounter;

            impl Drop for DropCounter {
                fn drop(&mut self) {
                    DROPS.fetch_add(1, Ordering::SeqCst);
                }
            }

            let mut rng = thread_rng();

            for _ in 0..runs {
                let steps = rng.gen_range(0..steps);
                let additional = rng.gen_range(0..additional);

                DROPS.store(0, Ordering::SeqCst);
                let q = ArrayQueue::new(50);

                scope(|scope| {
                    scope.spawn(|_| {
                        for _ in 0..steps {
                            while q.pop().is_none() {}
                        }
                    });

                    scope.spawn(|_| {
                        for _ in 0..steps {
                            while q.push(DropCounter).is_err() {
                                DROPS.fetch_sub(1, Ordering::SeqCst);
                            }
                        }
                    });
                })
                .unwrap();

                for _ in 0..additional {
                    q.push(DropCounter).unwrap();
                }

                assert_eq!(DROPS.load(Ordering::SeqCst), steps);
                drop(q);
                assert_eq!(DROPS.load(Ordering::SeqCst), steps + additional);
            }
        });
    }

    #[test]
    fn linearizable() {
        #[cfg(miri)]
        const COUNT: usize = 100;
        #[cfg(not(miri))]
        const COUNT: usize = 25_000;
        const THREADS: usize = 4;

        loom_crate::model(|| {
            let q = ArrayQueue::new(THREADS);

            scope(|scope| {
                for _ in 0..THREADS / 2 {
                    scope.spawn(|_| {
                        for _ in 0..COUNT {
                            while q.push(0).is_err() {}
                            q.pop().unwrap();
                        }
                    });

                    scope.spawn(|_| {
                        for _ in 0..COUNT {
                            if q.force_push(0).is_none() {
                                q.pop().unwrap();
                            }
                        }
                    });
                }
            })
            .unwrap();
        });
    }
}

mod seg_queue {
    use crossbeam_queue::SegQueue;

    use loom_crate::sync::atomic::{AtomicUsize, Ordering};
    use loom_crate::thread::scope;

    #[test]
    fn spsc() {
        #[cfg(miri)]
        const COUNT: usize = 100;
        #[cfg(not(miri))]
        const COUNT: usize = 100_000;

        loom_crate::model(|| {
            let q = SegQueue::new();

            scope(|scope| {
                scope.spawn(|_| {
                    for i in 0..COUNT {
                        loop {
                            if let Some(x) = q.pop() {
                                assert_eq!(x, i);
                                break;
                            }
                        }
                    }
                    assert!(q.pop().is_none());
                });
                scope.spawn(|_| {
                    for i in 0..COUNT {
                        q.push(i);
                    }
                });
            })
            .unwrap();
        });
    }

    #[test]
    fn mpmc() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 25_000;
        const THREADS: usize = 4;

        loom_crate::model(|| {
            let q = SegQueue::<usize>::new();
            let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

            scope(|scope| {
                for _ in 0..THREADS {
                    scope.spawn(|_| {
                        for _ in 0..COUNT {
                            let n = loop {
                                if let Some(x) = q.pop() {
                                    break x;
                                }
                            };
                            v[n].fetch_add(1, Ordering::SeqCst);
                        }
                    });
                }
                for _ in 0..THREADS {
                    scope.spawn(|_| {
                        for i in 0..COUNT {
                            q.push(i);
                        }
                    });
                }
            })
            .unwrap();

            for c in v {
                assert_eq!(c.load(Ordering::SeqCst), THREADS);
            }
        });
    }

    #[test]
    fn drops() {
        let runs: usize = if cfg!(miri) { 5 } else { 100 };
        let steps: usize = if cfg!(miri) { 50 } else { 10_000 };
        let additional: usize = if cfg!(miri) { 100 } else { 1_000 };

        loom_crate::model(|| {
            static DROPS: AtomicUsize = AtomicUsize::new(0);

            #[derive(Debug, PartialEq)]
            struct DropCounter;

            impl Drop for DropCounter {
                fn drop(&mut self) {
                    DROPS.fetch_add(1, Ordering::SeqCst);
                }
            }

            let mut rng = thread_rng();

            for _ in 0..runs {
                let steps = rng.gen_range(0..steps);
                let additional = rng.gen_range(0..additional);

                DROPS.store(0, Ordering::SeqCst);
                let q = SegQueue::new();

                scope(|scope| {
                    scope.spawn(|_| {
                        for _ in 0..steps {
                            while q.pop().is_none() {}
                        }
                    });

                    scope.spawn(|_| {
                        for _ in 0..steps {
                            q.push(DropCounter);
                        }
                    });
                })
                .unwrap();

                for _ in 0..additional {
                    q.push(DropCounter);
                }

                assert_eq!(DROPS.load(Ordering::SeqCst), steps);
                drop(q);
                assert_eq!(DROPS.load(Ordering::SeqCst), steps + additional);
            }
        });
    }
}
