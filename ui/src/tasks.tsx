// kb://tasks/ — resource monitor (task 2.10): memory/CPU of the browser's processes.
import { render } from 'solid-js/web';
import { createSignal, For, onCleanup, Show } from 'solid-js';
import { send } from './bridge';
import './base.css';
import './settings.css';
import './downloads.css';

interface Task {
  pid: number;
  name: string;
  kind: string;
  memory: number;
  cpu: number;
}

const mb = (b: number) => `${(b / 1048576).toFixed(0)} MB`;

function Tasks() {
  const [tasks, setTasks] = createSignal<Task[]>([]);
  const load = async () => setTasks(JSON.parse(await send({ cmd: 'tasks' })) as Task[]);
  void load();
  const timer = setInterval(() => void load(), 2000);
  onCleanup(() => clearInterval(timer));
  const total = () => tasks().reduce((n, t) => n + t.memory, 0);

  return (
    <main class="settings">
      <h1>Task manager</h1>
      <p class="muted">
        {tasks().length} processes · {mb(total())} in total
      </p>
      <ul class="list downloads">
        <For each={tasks()}>
          {(t) => (
            <li>
              <div class="info">
                <span class="name">
                  {t.kind} <span class="muted">({t.name}, pid {t.pid})</span>
                </span>
                <span class="muted">
                  {mb(t.memory)} · CPU {t.cpu.toFixed(1)} %
                </span>
              </div>
              <div class="actions">
                <Show when={t.kind !== 'browser'}>
                  <button class="plain" onClick={() => void send({ cmd: 'kill-process', pid: t.pid })}>
                    End process
                  </button>
                </Show>
              </div>
            </li>
          )}
        </For>
      </ul>
    </main>
  );
}

render(() => <Tasks />, document.getElementById('root')!);
