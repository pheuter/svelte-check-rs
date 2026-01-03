import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
    return {
        posts: [
            { id: 1, title: 'First Post', content: 'Hello world' },
            { id: 2, title: 'Second Post', content: 'Another post' }
        ],
        totalCount: 2
    };
};
