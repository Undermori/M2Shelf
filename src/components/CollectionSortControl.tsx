import {Select} from './Select';
import type { CollectionSort } from "../types/media";
import { useI18n } from "../lib/i18n";

export function CollectionSortControl({ value, onChange }: { value: CollectionSort; onChange: (value: CollectionSort) => void }) {
  const { t } = useI18n();
  const [field, direction] = value.split("-");
  const ascending = direction === "asc";
  return <div className="collection-sort-control">
    <Select className="collection-sort-select" aria-label={t("common.sort")} value={field} onChange={event => onChange(`${event.target.value}-${direction}` as CollectionSort)}>
      <option value="title">{t("sort.title")}</option><option value="added">{t("sort.added")}</option><option value="modified">{t("sort.modified")}</option><option value="watched">{t('comic.recentTitle')}</option>
    </Select>
    <button className="button secondary sort-direction" type="button" onClick={() => onChange(`${field}-${ascending ? "desc" : "asc"}` as CollectionSort)} aria-label={t(ascending ? "sort.ascending" : "sort.descending")} title={t(ascending ? "sort.descending" : "sort.ascending")}><span aria-hidden="true">{ascending ? "↑" : "↓"}</span>{t(ascending ? "sort.ascending" : "sort.descending")}</button>
  </div>;
}
