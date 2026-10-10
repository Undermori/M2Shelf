import {Select} from './Select';
import { useEffect, useMemo } from "react";
import type { MediaNode, UserTag } from "../types/media";
import { useI18n } from "../lib/i18n";
import { Icon } from "./Icon";

interface TagFilterProps {
  nodes: MediaNode[];
  value: number | null;
  onChange: (tagId: number | null) => void;
}

export function TagFilter({ nodes, value, onChange }: TagFilterProps) {
  const { language, t } = useI18n();
  const tags = useMemo(() => {
    const byId = new Map<number, UserTag>();
    for (const node of nodes) {
      for (const tag of node.userTags) byId.set(tag.id, tag);
    }
    return [...byId.values()].sort((left, right) =>
      left.name.localeCompare(right.name, language, { numeric: true, sensitivity: "base" }),
    );
  }, [language, nodes]);
  const selected = value != null && tags.some((tag) => tag.id === value) ? String(value) : "";

  useEffect(() => {
    if (value != null && !tags.some((tag) => tag.id === value)) onChange(null);
  }, [onChange, tags, value]);

  return (
    <label className="sort-field tag-filter-field">
      <Icon name="tag" />
      <span className="sr-only">{t("filter.tagsAria")}</span>
      <Select
        aria-label={t("filter.tagsAria")}
        onChange={(event) => onChange(event.target.value ? Number(event.target.value) : null)}
        value={selected}
      >
        <option value="">{t("filter.allTags")}</option>
        {tags.map((tag) => <option key={tag.id} value={tag.id}>{tag.name}</option>)}
      </Select>
    </label>
  );
}
