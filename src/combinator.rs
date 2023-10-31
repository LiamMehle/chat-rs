#[allow(dead_code)]
pub fn retry_if<T, E>(mut task: impl FnMut()->Result<T, E>, retry_predicate: impl Fn(&E)->bool) -> Result<T, E> {
	loop {
		match task() {
			Err(e) if retry_predicate(&e) => continue,
			x => break x
		}
	}
}

#[allow(dead_code)]
pub async fn async_retry_if<Fut, T, E>(task: impl Fn()->Fut, retry_predicate: impl Fn(&E)->bool) -> Result<T, E>
where Fut: futures::Future<Output=Result<T, E>> {
	loop {
		match task().await {
			Err(e) if retry_predicate(&e) => tokio::task::yield_now().await,
			x => break x
		}
	}
}