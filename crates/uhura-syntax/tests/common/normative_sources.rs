// Shared normative sources (design §4.6/§4.7/§6.1), include!()d by the
// parse and formatter test files.

const POST_CARD: &str = r#"component post-card

use port feed { type post-summary }

props {
  post: post-summary
  liked: bool
  like-pending: bool
}

emits {
  like-toggled(post: id, now-liked: bool)
  comments-requested(post: id)
  author-tapped(user: id)
}

<view class="post-card">
  <region label="View profile"
      on:activate={emit author-tapped(user: post.author.id)}>
    <view class="author-row">
      <img class="avatar" src={post.author.avatar.src} alt={post.author.avatar.alt} />
      <text class="username">{post.author.username}</text>
    </view>
  </region>

  {#match post.media}
    {:when image m}
      <region label="Like this post" supplementary
          on:activate-double={emit like-toggled(post: post.id, now-liked: true)}>
        <img class="media" src={m.image.src} alt={m.image.alt} />
      </region>
    {:when carousel c}
      <pager class="media" indicator="dots" label="Photo carousel">
        {#each c.slides as s (s.id)}
          <img src={s.src} alt={s.alt} />
        {/each}
      </pager>
    {:when video v}
      <view class="media video-fallback">
        <img src={v.poster.src} alt={v.poster.alt} />
        <view class="video-fallback-badge">
          <icon name="video-off" />
          <text>Video isn't supported in this preview</text>
        </view>
      </view>
  {/match}

  <view class="action-row">
    <button pressed={liked} busy={like-pending}
        label={if liked then "Unlike" else "Like"}
        on:press={emit like-toggled(post: post.id, now-liked: !liked)}>
      <icon name={if liked then "heart-filled" else "heart"} />
    </button>
    <button label="Comments"
        on:press={emit comments-requested(post: post.id)}>
      <icon name="comment" />
    </button>
  </view>

  <text class="likes">{to-text(post.like-count
      + (if liked && !post.viewer-has-liked then 1
         else if !liked && post.viewer-has-liked then 0 - 1
         else 0)) ++ " likes"}</text>
  <text class="caption">{post.caption}</text>
</view>

<style>
  .post-card { display: flex; flex-direction: column; gap: var(--space-2); }
  .post-card .avatar { inline-size: 32px; border-radius: var(--radius-full); }
</style>
"#;

const FEED_STORE: &str = r#"page

use component post-card
use surface comments-sheet
use port feed {
  projection feed-page, projection viewer,
  command like-post, command unlike-post,
  command load-next-page, command reload
}

store {
  state {
    like-overlay: map[id]bool = {}
    like-pending: map[id]bool = {}
    load-pending: bool = false
    notice: text? = none
  }

  // like / unlike: guard-ordered multi-handler dispatch
  on like-toggled(post: id, now-liked: bool)
      when now-liked && !(like-pending[post] ?? false) {
    set like-overlay[post] = true
    set like-pending[post] = true
    send like-post(post: post)
  }
  on like-post.ok(tag, cmd) {
    set like-pending[cmd.post] = none
    set like-overlay[cmd.post] = none
  }
  on like-post.err(tag, cmd, refusal) {
    set like-pending[cmd.post] = none
    set notice = "Couldn't like this post. Try again."
  }
  on feed-near-end()
      when !load-pending && feed-page.has-more && feed-page.cursor != none {
    set load-pending = true
    send load-next-page(cursor: feed-page.cursor)
  }
  on comments-requested(post: id) { open-surface comments-sheet(post: post) }
  on author-tapped(user: id) { navigate profile(user: user) }
  on submit-requested() when draft != "" {
    send add-comment(post: post, body: draft) as t
    set pending-appends[t] = draft
    set draft = ""
  }
  on dismiss-requested() { dismiss }
  on back-tapped() { navigate back }
}

<view class="screen feed-page">
  {#if notice != none}
    <notice-bar text={notice ?? ""} on:dismissed={emit notice-dismissed()} />
  {/if}
  {#match feed-page}
    {:when loading}
      <view class="fill-center"><text class="muted">Loading your feed…</text></view>
    {:when ready f}
      <scroll class="feed-scroll" on:near-end={emit feed-near-end()}>
        <view role="list" class="post-list">
          {#each f.posts as p (p.id)}
            <post-card post={p}
                liked={like-overlay[p.id] ?? p.viewer-has-liked}
                like-pending={like-pending[p.id] ?? false}
                on:like-toggled on:comments-requested on:author-tapped />
          {/each}
        </view>
        {#if !f.has-more}
          <text class="muted">You're all caught up.</text>
        {/if}
      </scroll>
  {/match}
  <bottom-nav current="feed" on:tab-selected />
</view>
"#;

const FEED_EXAMPLES: &str = r#"use fixture standard

example loading {
  note "cold start — nothing delivered yet"
}

example first-page default {
  projection feed.viewer = fixture.users.mira
  projection feed.feed-page = fixture.feed.page-1
}

example like-pending {
  from first-page
  events [ like-toggled(post: "post-lena-glaze", now-liked: true) ]
  note "optimistic heart + count while like-post is in flight"
}

example comments-open {
  from first-page
  projection comments.for-post("post-lena-glaze") = fixture.comments.lena-glaze
  events [ comments-requested(post: "post-lena-glaze") ]
}

example appended {
  from first-page
  events [
    feed-near-end()
    projection feed.feed-page = fixture.feed.pages-1-2
    outcome load-next-page.ok()
  ]
}
"#;
