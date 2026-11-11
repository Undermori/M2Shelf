import type {ScanChanges as Changes} from '../types/media';
import {useI18n} from '../lib/i18n';

export function ScanChanges({changes,onAdded}:{changes?:Changes|null;onAdded?:()=>void}) {
  const {t,number}=useI18n();
  if(!changes)return null;
  return <div className="scan-changes" aria-label={t('delta.title')}>
    <span>{t('delta.added')} <strong>{number(changes.added)}</strong></span>
    <span>{t('delta.updated')} <strong>{number(changes.updated)}</strong></span>
    <span>{t('delta.unchanged')} <strong>{number(changes.unchanged)}</strong></span>
    <span>{t('delta.removed')} <strong>{changes.removed===null?t('delta.unconfirmed'):number(changes.removed)}</strong></span>
    <span>{t('delta.errors')} <strong>{number(changes.errors)}</strong></span>
    <small>{t('delta.unit')}{!changes.complete?` · ${t('delta.incomplete')}`:''}</small>
    {onAdded&&changes.added>0&&<button type="button" className="button secondary" onClick={onAdded}>{t('delta.viewAdded')}</button>}
  </div>;
}
