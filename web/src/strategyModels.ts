import type { StrategyModel } from "./api";

export function selectedStrategyModels(options: StrategyModel[], selected: string[] | null): string[] {
  const requested = new Set(selected ?? options.filter((item) => item.selected).map((item) => item.model));
  return [...new Set(options.filter((item) => item.profile_id && requested.has(item.model)).map((item) => item.model))];
}

export function toggleStrategyModel(options: StrategyModel[], selected: string[], model: string, checked: boolean): string[] {
  return selectedStrategyModels(options, checked ? [...selected, model] : selected.filter((item) => item !== model));
}
