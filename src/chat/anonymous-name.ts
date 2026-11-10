const adjectives = ['Тихий', 'Сонный', 'Весёлый', 'Добрый', 'Смелый', 'Ловкий', 'Рыжий', 'Мудрый', 'Мирный', 'Важный', 'Лунный', 'Звёздный', 'Снежный', 'Уютный', 'Шустрый', 'Пушистый'] as const;
const animals = ['Енот', 'Лис', 'Барсук', 'Бобр', 'Волк', 'Тигр', 'Лев', 'Медведь', 'Лось', 'Олень', 'Заяц', 'Ёж', 'Хомяк', 'Кот', 'Кит', 'Пингвин'] as const;

export function createAnonymousName(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(2));
  // Each 16-word list uses four uniformly random bits; this is a local cosmetic alias.
  return `${adjectives[bytes[0] & 15]} ${animals[bytes[1] & 15]}`;
}
