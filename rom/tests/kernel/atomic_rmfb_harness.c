/* Userspace fault-injection harness for the actual backported C function.
 * This models DRM transaction/ownership contracts, not the SDE hardware.
 */
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#define BIT(x) (1U << (x))
#define IS_ERR(p) ((uintptr_t)(p) > (uintptr_t)-4096)
#define PTR_ERR(p) ((int)(intptr_t)(p))
struct drm_device;
struct drm_crtc;
struct drm_modeset_acquire_ctx { bool locked; };
struct drm_framebuffer { struct drm_device *dev; };
struct drm_plane_state { struct drm_framebuffer *fb; struct drm_crtc *crtc; };
struct drm_plane {
    int index;
    struct drm_plane_state *state;
    struct drm_framebuffer *fb, *old_fb;
    struct drm_crtc *crtc;
};
struct drm_crtc_state { bool active, mode; };
struct drm_crtc { struct drm_plane *primary; struct drm_crtc_state state; };
struct drm_connector { int unused; };
struct drm_connector_state { struct drm_crtc *crtc; };
struct drm_device {
    struct drm_plane planes[3];
    struct drm_plane_state current[3];
    struct drm_crtc crtc;
    struct drm_connector connector;
    struct drm_connector_state conn;
};
struct drm_atomic_state {
    struct drm_device *dev;
    struct drm_modeset_acquire_ctx *acquire_ctx;
    bool selected[3], connectors;
    struct drm_plane_state planes[3];
    struct drm_crtc_state crtc;
    struct drm_connector_state conn;
};
static int allocations, frees, commits, backoffs, clears, contexts, fault;
static bool require_primary, reject_commit, fail_alloc, fail_late_plane;
#define drm_for_each_plane(p,d) \
    for ((p) = (d)->planes; (p) < (d)->planes + 3; ++(p))
#define for_each_connector_in_state(s,c,cs,i) \
    for ((i)=0; (s)->connectors && (i)<1 && \
         (((c)=&(s)->dev->connector), ((cs)=&(s)->conn), true); ++(i))
static int drm_plane_index(struct drm_plane *p) { return p->index; }
static void drm_modeset_acquire_init(struct drm_modeset_acquire_ctx *c, int flags) {
    (void)flags; c->locked=false; ++contexts;
}
static void drm_modeset_drop_locks(struct drm_modeset_acquire_ctx *c) { c->locked=false; }
static void drm_modeset_acquire_fini(struct drm_modeset_acquire_ctx *c) {
    assert(!c->locked); --contexts;
}
static int drm_modeset_lock_all_ctx(struct drm_device *d, struct drm_modeset_acquire_ctx *c) {
    (void)d;
    if (fault==1) { fault=0; return -EDEADLK; }
    c->locked=true; return 0;
}
static void drm_modeset_backoff(struct drm_modeset_acquire_ctx *c) {
    c->locked=false; ++backoffs;
}
static struct drm_atomic_state *drm_atomic_state_alloc(struct drm_device *d) {
    struct drm_atomic_state *s;
    if (fail_alloc) return NULL;
    s=calloc(1,sizeof(*s)); assert(s); ++allocations;
    s->dev=d; s->crtc=d->crtc.state; s->conn=d->conn; return s;
}
static void drm_atomic_state_clear(struct drm_atomic_state *s) {
    assert(s); memset(s->selected,0,sizeof(s->selected));
    s->connectors=false; s->crtc=s->dev->crtc.state; s->conn=s->dev->conn; ++clears;
}
static void drm_atomic_state_free(struct drm_atomic_state *s) { assert(s); ++frees; free(s); }
static struct drm_plane_state *drm_atomic_get_plane_state(struct drm_atomic_state *s,
                                                         struct drm_plane *p) {
    assert(s->acquire_ctx->locked);
    if (p->index==1 && (fault==2 || fail_late_plane)) {
        int e=fail_late_plane ? -ENOMEM : -EDEADLK; fault=0;
        return (void *)(intptr_t)e;
    }
    s->selected[p->index]=true; s->planes[p->index]=*p->state;
    return &s->planes[p->index];
}
static struct drm_crtc_state *drm_atomic_get_existing_crtc_state(
        struct drm_atomic_state *s, struct drm_crtc *c) { (void)c; return &s->crtc; }
static int drm_atomic_add_affected_connectors(struct drm_atomic_state *s, struct drm_crtc *c) {
    (void)c; s->connectors=true; return 0;
}
static int drm_atomic_set_mode_for_crtc(struct drm_crtc_state *c, void *mode) {
    assert(!mode); c->mode=false; return 0;
}
static void drm_atomic_set_fb_for_plane(struct drm_plane_state *p, struct drm_framebuffer *f) { p->fb=f; }
static int drm_atomic_set_crtc_for_plane(struct drm_plane_state *p, struct drm_crtc *c) { p->crtc=c; return 0; }
static int drm_atomic_set_crtc_for_connector(struct drm_connector_state *s, struct drm_crtc *c) { s->crtc=c; return 0; }
static int drm_atomic_commit(struct drm_atomic_state *s) {
    ++commits;
    if (fault==3) { fault=0; return -EDEADLK; }
    if (fault==4) return -EIO;
    if (reject_commit || (require_primary && s->crtc.active &&
                         s->selected[0] && !s->planes[0].fb)) return -EINVAL;
    for (int i=0;i<3;++i) if(s->selected[i]) *s->dev->planes[i].state=s->planes[i];
    s->dev->crtc.state=s->crtc; s->dev->conn=s->conn;
    drm_atomic_state_free(s); /* The 4.9 driver consumes a successful state. */
    return 0;
}
static void drm_atomic_clean_old_fb(struct drm_device *d, unsigned mask, int ret) {
    for (int i=0;i<3;++i) if(mask & BIT(i)) {
        if(!ret) { d->planes[i].fb=d->current[i].fb; d->planes[i].crtc=d->current[i].crtc; }
        d->planes[i].old_fb=NULL;
    }
}
/* The runner inserts the exact patched function here, before main(). */
#include "atomic_remove_fb_under_test.c"
static void setup(struct drm_device *d, struct drm_framebuffer *f, int mask) {
    memset(d,0,sizeof(*d)); f->dev=d;
    d->crtc.primary=&d->planes[0]; d->crtc.state=(struct drm_crtc_state){true,true};
    d->conn.crtc=&d->crtc;
    for(int i=0;i<3;++i) {
        d->planes[i].index=i; d->planes[i].state=&d->current[i];
        if(mask & BIT(i)) {
            d->current[i]=(struct drm_plane_state){f,&d->crtc};
            d->planes[i].fb=f; d->planes[i].crtc=&d->crtc;
        }
    }
    allocations=frees=commits=backoffs=clears=contexts=fault=0;
    require_primary=reject_commit=fail_alloc=fail_late_plane=false;
}
static void balanced(struct drm_device *d) {
    assert(allocations==frees); assert(contexts==0);
    for(int i=0;i<3;++i) assert(!d->planes[i].old_fb);
}
int main(void) {
    struct drm_device d; struct drm_framebuffer f;
    /* Removing primary, overlay, or shared FB must preserve route and power. */
    for(int mask=0;mask<8;++mask) {
        setup(&d,&f,mask); assert(drm_atomic_remove_fb(&f)==0);
        assert(d.crtc.state.active && d.crtc.state.mode && d.conn.crtc==&d.crtc);
        for(int i=0;i<3;++i) assert(!d.current[i].fb && !d.planes[i].fb);
        assert(commits==(mask ? 1:0)); balanced(&d);
    }
    /* Drivers that require primary retain the upstream disable fallback. */
    setup(&d,&f,1); require_primary=true; assert(drm_atomic_remove_fb(&f)==0);
    assert(commits==2 && !d.crtc.state.active && !d.crtc.state.mode && !d.conn.crtc); balanced(&d);
    /* Invalid paired-plane removal must not partially change display state. */
    setup(&d,&f,2); reject_commit=true; assert(drm_atomic_remove_fb(&f)==-EINVAL);
    assert(commits==2 && d.crtc.state.active && d.conn.crtc==&d.crtc);
    assert(d.current[1].fb==&f && d.planes[1].fb==&f); balanced(&d);
    /* Allocation, prepare and commit failures must preserve live buffers. */
    setup(&d,&f,1); fail_alloc=true; assert(drm_atomic_remove_fb(&f)==-ENOMEM); balanced(&d);
    setup(&d,&f,3); fail_late_plane=true; assert(drm_atomic_remove_fb(&f)==-ENOMEM);
    assert(!commits && d.current[0].fb==&f && d.planes[0].fb==&f); balanced(&d);
    setup(&d,&f,1); fault=4; assert(drm_atomic_remove_fb(&f)==-EIO);
    assert(commits==1 && d.current[0].fb==&f && d.planes[0].fb==&f); balanced(&d);
    /* Lock, mid-prepare and commit deadlocks clear state before retrying. */
    for(int n=1;n<=3;++n) {
        setup(&d,&f,3); fault=n; assert(drm_atomic_remove_fb(&f)==0);
        assert(backoffs==1 && clears==1 && d.crtc.state.active); balanced(&d);
    }
    puts("16 atomic RMFB transaction cases passed");
}
