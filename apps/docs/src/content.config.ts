// Starlight content collection. `generated` and `breadcrumbs` are written by `fw site gen` into every page.
import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';
import { z } from 'astro/zod';

export const collections = {
  docs: defineCollection({
    loader: docsLoader(),
    schema: docsSchema({
      extend: z.object({
        generated: z.string().optional(),
        breadcrumbs: z.array(z.object({ name: z.string(), path: z.string() })).optional(),
      }),
    }),
  }),
};
