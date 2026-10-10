import type {TextReaderSettings} from '../types/textReader';
import {defaultTextReaderSettings} from '../types/textReader';
import {useI18n, type MessageKey} from '../lib/i18n';
import {Select} from './Select';

export function TextReaderControls({value, onChange}: {value: TextReaderSettings; onChange: (value: TextReaderSettings) => void}) {
  const {t} = useI18n();
  const update = <K extends keyof TextReaderSettings>(key: K, next: TextReaderSettings[K]) => onChange({...value, [key]: next});
  const range = (key: keyof TextReaderSettings, label: MessageKey, min: number, max: number, step = 1, suffix = '') => <label className="reader-range" key={key}><span>{t(label)}<output>{value[key]}{suffix}</output></span><input aria-label={t(label)} type="range" min={min} max={max} step={step} value={Number(value[key])} onChange={e => update(key, Number(e.target.value))}/></label>;
  return <div className="text-reader-controls">
    <label><span>{t('comic.mode')}</span><Select aria-label={t('comic.mode')} value={value.mode} onChange={e => update('mode', e.target.value as TextReaderSettings['mode'])}><option value="PAGED">{t('comic.paged')}</option><option value="SCROLL">{t('comic.scroll')}</option></Select></label>
    <h3>{t('text.typography')}</h3>
    <label><span>{t('text.fontFamily')}</span><Select aria-label={t('text.fontFamily')} value={value.fontFamily} onChange={e => update('fontFamily', e.target.value as TextReaderSettings['fontFamily'])}><option value="SYSTEM">{t('text.systemFont')}</option><option value="SERIF">{t('text.serif')}</option><option value="MONO">{t('text.mono')}</option></Select></label>
    {range('fontSize', 'text.fontSize', 12, 40, 1, 'px')}
    {range('fontWeight', 'text.fontWeight', 300, 800, 100)}
    <label><span>{t('text.align')}</span><Select aria-label={t('text.align')} value={value.alignment} onChange={e => update('alignment', e.target.value as TextReaderSettings['alignment'])}>{(['left','center','right','justify'] as const).map(v => <option value={v} key={v}>{t(`text.${v}`)}</option>)}</Select></label>
    <label className="switch-field settings-switch-row"><span>{t('text.italic')}</span><input type="checkbox" aria-label={t('text.italic')} checked={value.italic} onChange={e => update('italic', e.target.checked)}/><i/></label>
    {range('letterSpacing', 'text.letterSpacing', 0, .2, .01, 'em')}{range('wordSpacing', 'text.wordSpacing', 0, .5, .01, 'em')}
    {range('lineHeight', 'text.lineHeight', 1.2, 2.6, .1)}{range('paragraphSpacing', 'text.paragraphSpacing', 0, 2, .1, 'em')}
    <h3>{t('comic.layout')}</h3>
    {range('maxWidth', 'text.maxWidth', 400, 1200, 20, 'px')}{range('horizontalMargin', 'text.horizontalMargin', 12, 80, 2, 'px')}{range('verticalMargin', 'text.verticalMargin', 12, 80, 2, 'px')}
    <h3>{t('comic.background')}</h3>
    <Select aria-label={t('comic.background')} value={value.theme} onChange={e => update('theme', e.target.value as TextReaderSettings['theme'])}>{(['APP','PAPER','SEPIA','NIGHT','CUSTOM'] as const).map(v => <option value={v} key={v}>{t(`text.theme${v}`)}</option>)}</Select>
    {value.theme === 'CUSTOM' && <div className="reader-colors"><label><span>{t('comic.background')}</span><input type="color" aria-label={t('comic.background')} value={value.backgroundColor} onChange={e => update('backgroundColor', e.target.value)}/></label><label><span>{t('text.textColor')}</span><input type="color" aria-label={t('text.textColor')} value={value.textColor} onChange={e => update('textColor', e.target.value)}/></label></div>}
    <details className="text-color-filters"><summary>{t('text.filters')}</summary>{range('brightness','text.brightness',50,150,5,'%')}{range('contrast','text.contrast',50,150,5,'%')}{range('saturation','text.saturation',0,150,5,'%')}{range('sepia','text.sepia',0,100,5,'%')}{range('hue','text.hue',0,360,5,'°')}<label className="switch-field settings-switch-row"><span>{t('text.negative')}</span><input aria-label={t('text.negative')} type="checkbox" checked={value.negative} onChange={e => update('negative',e.target.checked)}/><i/></label></details>
    <button className="button secondary" type="button" onClick={() => onChange({...defaultTextReaderSettings})}>{t('text.reset')}</button>
  </div>;
}
