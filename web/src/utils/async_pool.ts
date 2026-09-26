export async function* asyncPool<IN, OUT>(
  poolLimit: number,
  array: Array<IN>,
  iteratorFn: (generator: IN, array?: Array<IN>) => Promise<OUT>,
): AsyncIterableIterator<OUT> {
  const executing = new Set<Promise<OUT>>()
  for (const item of array) {
    const p: Promise<OUT> = iteratorFn(item, array).finally(() => executing.delete(p))
    executing.add(p)
    if (executing.size >= poolLimit) {
      yield await Promise.race(executing)
    }
  }
  while (executing.size > 0) {
    yield await Promise.race(executing)
  }
}
