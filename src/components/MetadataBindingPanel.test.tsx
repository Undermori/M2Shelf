// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { I18nProvider } from '../lib/i18n';
import { MetadataBindingPanel } from './MetadataBindingPanel';

afterEach(cleanup);

it.each(['Bangumi', 'TMDb'] as const)('separates %s source, bound identity and one external title action', provider => {
  const open = vi.fn();
  render(<I18nProvider><MetadataBindingPanel provider={provider} label="legacy source label" subjectId={489} title="A long source title / 長い作品名 / 긴 작품 제목" openLabel="Open official title" onOpen={open}>
    <button type="button">Change binding</button>
  </MetadataBindingPanel></I18nProvider>);
  expect(screen.getByText(provider)).toBeTruthy();
  expect(screen.getByText('已绑定 · #489')).toBeTruthy();
  expect(screen.queryByText('legacy source label')).toBeNull();
  expect(document.querySelectorAll('.binding-subject-link svg')).toHaveLength(1);
  expect(document.querySelector('.binding-logo svg rect')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Open official title' }));
  expect(open).toHaveBeenCalledOnce();
});

it('keeps the unbound guidance and cover warning without inventing a source link', () => {
  render(<I18nProvider><MetadataBindingPanel provider="Bangumi" label="Bangumi" title="No online subject linked" error="Cover remains unavailable"><button type="button">Search and add</button></MetadataBindingPanel></I18nProvider>);
  expect(screen.getAllByText('Bangumi')).toHaveLength(1);
  expect(screen.getByText('No online subject linked')).toBeTruthy();
  expect(screen.getByText('Cover remains unavailable')).toBeTruthy();
  expect(document.querySelector('.binding-subject-link')).toBeNull();
});
