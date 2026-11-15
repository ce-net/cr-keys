// KeyList, the face of cell:cr.keys@2. Metadata of every key the viewer's chain lets through; never a value (the
// row type has no field for one). Imports only ui/gen and the kit. Renew is drawn disabled until approvals land.
import { KeyList } from '../gen/CrKeys.ui';
import type { KeyRow, RotationState } from '../gen/CrKeys';
import { show } from '@ui/rt';
import { clear, date, exhaustive, h, num } from '@ui/kit/dom';
import { empty, failed, loading, sourcesLine } from '@ui/kit/faces';

export function rotationText(r: RotationState): [string, string] {
  switch (r.tag) {
    case 'Fresh':
      return ['fresh', 'g'];
    case 'DueSoon':
      return [`due in ${r.days} d`, 'w'];
    case 'Overdue':
      return [`overdue ${r.days} d`, 'r'];
    case 'Rotating':
      return ['rotating', 'w'];
    case 'Failed':
      return [`check failed: ${r.why}`, 'r'];
    case 'Manual':
      return ['manual', ''];
    default:
      return exhaustive(r);
  }
}

function row(k: KeyRow, renew: (name: string) => void): HTMLElement {
  const [rt, rc] = rotationText(k.rotation);
  const left = k.days_left === null ? '-' : `${k.days_left} d`;
  return h(
    'tr',
    {},
    h('td', {}, h('b', { text: k.name }), h('div', { class: 'sub', text: [k.service, k.purpose].filter((s) => s !== '').join(' · ') || '-' })),
    h('td', {}, h('span', { class: `st ${rc}` }, h('i'), rt)),
    h('td', { class: 'mono', text: left }),
    h('td', { class: 'mono', text: k.expires_at === null ? '-' : date(Number(k.expires_at) * 1000) }),
    // a Part: absent on the wire for a viewer without cr.keys:holders, so the cell shows nothing, not "0"
    h('td', { class: 'hide-sm', text: show(k.holders, (hs) => (hs.length > 0 ? hs.map((x) => x.name).join(', ') : '-')) ?? '' }),
    h('td', { class: 'mono hide-sm', text: `v${num(k.version)} ${k.sha256_8}` }),
    h('td', { class: 'mono hide-sm', text: k.last_check === '' ? '-' : `${k.last_check} ${date(Number(k.last_check_at) * 1000)}` }),
    h('td', {}, h('button', { class: 'btn quiet', disabled: true, title: 'Renewing arrives with passkey approvals', text: 'Renew', onclick: () => renew(k.name) })),
  );
}

const CHOICES: readonly (readonly [string, number | null])[] = [['All', null], ['Due in 30 days', 30], ['Due in 7 days', 7]];

export default KeyList.render((v) => {
  const body = h('div', {}, loading());
  const search = h('input', { class: 'search', label: 'Find a key', attrs: { type: 'search', placeholder: 'Find a key, service or purpose' } });
  const pills = h('div', { class: 'pills', role: 'group', label: 'Due within' });
  const drawPills = (): void => {
    clear(
      pills,
      ...CHOICES.map(([t, d]) =>
        h('button', { pressed: v.props.due_within_days === d, text: t, onclick: () => { v.setProps({ due_within_days: d }); drawPills(); } }),
      ),
    );
  };
  search.addEventListener('input', () => v.setProps({ q: search.value.trim() === '' ? null : search.value.trim() }));
  drawPills();
  v.draw((d, why) => {
    if (d.rows === undefined) return clear(body, why === null ? loading() : failed(why));
    const rows = [...d.rows].sort((a, b) => Number((a.days_left ?? 1_000_000n) - (b.days_left ?? 1_000_000n)) || a.name.localeCompare(b.name));
    const over = rows.filter((k) => k.rotation.tag === 'Overdue' || k.rotation.tag === 'Failed').length;
    const soon = rows.filter((k) => k.rotation.tag === 'DueSoon').length;
    clear(
      body,
      h(
        'div',
        { class: 'stats' },
        h('div', { class: 'glass ticks stat' }, h('div', { class: 'label' }, h('i'), 'Keys'), h('div', { class: 'num acc', text: num(rows.length) }), h('div', { class: 'sub', text: 'metadata only, never a value' })),
        h('div', { class: 'glass stat' }, h('div', { class: 'label' }, h('i', { class: 'r' }), 'Overdue or failed'), h('div', { class: 'num', text: num(over) }), h('div', { class: 'sub', text: 'every share ended, or the last check failed' })),
        h('div', { class: 'glass stat' }, h('div', { class: 'label' }, h('i'), 'Due in 30 days'), h('div', { class: 'num', text: num(soon) }), h('div', { class: 'sub', text: 'the autopilot renews these' })),
      ),
      rows.length === 0
        ? empty(v.props.q === null ? 'No keys here.' : `No key matches "${v.props.q}".`)
        : h(
            'div',
            { class: 'glass tablewrap' },
            h(
              'table',
              {},
              h('thead', {}, h('tr', {}, ...['Key', 'Rotation', 'Left', 'Ends', 'Holders', 'Version', 'Last check', ''].map((t, i) => h('th', { class: i >= 4 && i <= 6 ? 'hide-sm' : '', text: t })))),
              h('tbody', {}, ...rows.slice(0, 300).map((k) => row(k, v.emit.renew))),
            ),
          ),
      rows.length > 300 ? h('div', { class: 'sub', text: `First 300 of ${num(rows.length)}; narrow the search.` }) : null,
      why === null ? null : failed(why),
      sourcesLine(d.sources, why === null),
    );
  });
  return h('section', { class: 'panel', attrs: { 'data-view': 'KeyList' } }, h('div', { class: 'toolbar' }, search, pills), body);
});
