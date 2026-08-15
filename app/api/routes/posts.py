from fastapi import APIRouter, Depends, HTTPException, status
from sqlalchemy import select
from sqlalchemy.orm import Session

from app.core.database import get_db
from app.core.security import get_current_user
from app.models.post import Post
from app.models.user import Follow, User
from app.schemas.post import PostCreate, PostOut

router = APIRouter(prefix="/posts", tags=["posts"])


@router.post("", response_model=PostOut, status_code=status.HTTP_201_CREATED)
def create_post(
    payload: PostCreate,
    current_user: User = Depends(get_current_user),
    db: Session = Depends(get_db),
) -> Post:
    content = payload.content.strip()
    if not content:
        raise HTTPException(status_code=400, detail="Post content cannot be empty")
    if len(content) > 280:
        raise HTTPException(status_code=400, detail="Post exceeds 280 characters")

    post = Post(author_id=current_user.id, content=content)
    db.add(post)
    db.commit()
    db.refresh(post)
    return post


@router.get("/feed", response_model=list[PostOut])
def read_feed(
    following: bool = False,
    current_user: User = Depends(get_current_user),
    db: Session = Depends(get_db),
):
    stmt = select(Post).order_by(Post.created_at.desc())
    if following:
        followed_ids = select(Follow.following_id).where(
            Follow.follower_id == current_user.id
        )
        stmt = stmt.where(Post.author_id.in_(followed_ids))
    return db.scalars(stmt).all()


@router.get("/{post_id}", response_model=PostOut)
def read_post(post_id: int, db: Session = Depends(get_db)) -> Post:
    post = db.get(Post, post_id)
    if post is None:
        raise HTTPException(status_code=404, detail="Post not found")
    return post


@router.delete("/{post_id}", status_code=status.HTTP_204_NO_CONTENT)
def delete_post(
    post_id: int,
    current_user: User = Depends(get_current_user),
    db: Session = Depends(get_db),
) -> None:
    post = db.get(Post, post_id)
    if post is None:
        raise HTTPException(status_code=404, detail="Post not found")
    if post.author_id != current_user.id:
        raise HTTPException(status_code=403, detail="Not allowed to delete this post")
    db.delete(post)
    db.commit()


@router.get("/user/{user_id}", response_model=list[PostOut])
def read_user_posts(user_id: int, db: Session = Depends(get_db)):
    user = db.get(User, user_id)
    if user is None:
        raise HTTPException(status_code=404, detail="User not found")
    stmt = (
        select(Post).where(Post.author_id == user_id).order_by(Post.created_at.desc())
    )
    return db.scalars(stmt).all()
