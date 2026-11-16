import type { ReactNode } from 'react';
import { useI18n } from '../lib/i18n';
import { Icon } from './Icon';

interface MetadataBindingPanelProps {
  provider: 'Bangumi' | 'TMDb';
  label: string;
  subjectId?: number;
  title: string;
  openLabel?: string;
  onOpen?: () => void;
  error?: ReactNode;
  children: ReactNode;
}

export function MetadataBindingPanel({ provider, label, subjectId, title, openLabel, onOpen, error, children }: MetadataBindingPanelProps) {
  const { t } = useI18n();
  return <div className="binding-panel metadata-binding" data-provider={provider}>
    <div className="binding-layout">
      <span className="binding-logo"><Icon name={provider === 'Bangumi' ? 'bangumi' : 'tmdb'} /></span>
      <div className="binding-copy">
        <div className="binding-meta">
          <span className="binding-provider">{provider}</span>
          {subjectId !== undefined ? <small>{t('match.boundId', { id: subjectId })}</small> : onOpen ? <small>{label}</small> : null}
        </div>
        {onOpen ? <button className="binding-subject-link" type="button" onClick={onOpen} title={`${title} · ${openLabel ?? ''}`} aria-label={openLabel}>
          <span className="binding-title">{title}</span><Icon className="binding-open-icon" name="external" />
        </button> : <span className="binding-unbound">{title}</span>}
        {error && <em className="binding-error">{error}</em>}
      </div>
      <div className="binding-actions">{children}</div>
    </div>
  </div>;
}
