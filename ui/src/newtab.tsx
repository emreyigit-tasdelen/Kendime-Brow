// kb://newtab/ — start page (task 1.7). Search goes through the bridge `navigate` command.
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import { send } from './bridge';
import './base.css';
import './newtab.css';

function NewTab() {
  const [text, setText] = createSignal('');
  return (
    <main class="newtab">
      <h1>Kendime-Brow</h1>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (text().trim()) void send({ cmd: 'navigate', text: text() });
        }}
      >
        <input
          type="text"
          autofocus
          spellcheck={false}
          placeholder="Search or enter address"
          value={text()}
          onInput={(e) => setText(e.currentTarget.value)}
        />
      </form>
    </main>
  );
}

render(() => <NewTab />, document.getElementById('root')!);
