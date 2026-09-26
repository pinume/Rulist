type Events = {
  to: string
  tool: string
  pathname: string
  upload_files: File[]
}

type Handler<T = any> = (event: T) => void

class Bus {
  private all = new Map<keyof Events, Set<Handler>>()

  on<Key extends keyof Events>(type: Key, handler: Handler<Events[Key]>) {
    let handlers = this.all.get(type)
    if (!handlers) {
      handlers = new Set()
      this.all.set(type, handlers)
    }
    handlers.add(handler as Handler)
  }

  off<Key extends keyof Events>(type: Key, handler: Handler<Events[Key]>) {
    this.all.get(type)?.delete(handler as Handler)
  }

  emit<Key extends keyof Events>(type: Key, event: Events[Key]) {
    this.all.get(type)?.forEach((h) => h(event))
  }
}

export const bus = new Bus()
