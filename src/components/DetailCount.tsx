import {useI18n, type MessageKey} from '../lib/i18n';

/** Keep the number prominent without changing each locale's sentence order. */
export function DetailCount({count, message}: {count: number; message: MessageKey}) {
  const {t, number} = useI18n();
  const [before, after] = t(message, {count: '\uFFF0'}).split('\uFFF0');
  return <span>{before}<strong>{number(count)}</strong>{after}</span>;
}
